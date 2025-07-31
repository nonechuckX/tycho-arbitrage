//! Bundle relay client for submitting bundles to MEV relayers.
//! 
//! This module handles the networking aspects of bundle submission,
//! including JSON-RPC communication and signature handling.

use crate::bundle::{Bundle, BundleSubmission};
use crate::config::ArbitrageConfig;
use crate::errors::{BundleError, Result};
use alloy::primitives::keccak256;
use alloy::signers::{local::PrivateKeySigner, Signer};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Parameters for the eth_sendBundle JSON-RPC method.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EthSendBundleParams {
    pub txs: Vec<String>,
    pub block_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub builders: Option<Vec<String>>,
}

impl EthSendBundleParams {
    /// Create new bundle parameters for a specific relayer.
    pub fn new(bundle: &Bundle, relayer: &str) -> Self {
        let builder_params = crate::utils::builder_params(relayer);

        Self {
            txs: bundle.transactions().clone().to_vec(),
            block_number: format!("0x{:x}", bundle.target_block()),
            builders: builder_params,
        }
    }
}

/// Generic JSON-RPC request structure.
#[derive(Serialize, Debug)]
pub struct JsonRpcRequest<T> {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: Vec<T>,
}

impl<EthSendBundleParams> JsonRpcRequest<EthSendBundleParams> {
    /// Create a new eth_sendBundle request.
    pub fn new(params: EthSendBundleParams) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "eth_sendBundle".to_string(),
            params: vec![params],
        }
    }
}

/// Generic JSON-RPC response structure.
#[derive(Deserialize, Debug)]
pub struct JsonRpcResponse<T> {
    pub result: Option<T>,
    pub error: Option<JsonRpcError>,
}

/// Response from eth_sendBundle method.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EthSendBundleResponse {
    pub bundle_hash: String,
}

/// JSON-RPC error structure.
#[derive(Deserialize, Debug)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

/// Client for communicating with MEV relayers.
pub struct RelayClient {
    http_client: HttpClient,
    identity_signer: PrivateKeySigner,
    relayer_urls: Vec<String>,
}

impl RelayClient {
    /// Create a new RelayClient from configuration.
    /// 
    /// # Arguments
    /// 
    /// * `config` - The arbitrage configuration containing relayer settings
    /// * `identity_key` - The private key for Flashbots identification 
    pub fn from_config(config: &ArbitrageConfig, identity_key: &str) -> Result<Self> {
        let identity_signer = identity_key.parse::<PrivateKeySigner>()
            .map_err(|e| BundleError::InvalidPrivateKey {
                message: format!("Failed to parse identity key: {}", e),
            })?;

        let http_client = HttpClient::builder()
            .timeout(Duration::from_millis(config.relayer.timeout_ms))
            .build()?;

        Ok(Self {
            http_client,
            identity_signer,
            relayer_urls: config.relayer_urls().to_vec(),
        })
    }


    /// Submit a bundle to all configured relayers concurrently.
    /// 
    /// Returns a vector of submission results, one for each relayer.
    pub async fn submit_bundle(&self, bundle: &Bundle) -> Vec<BundleSubmission> {
        use futures::future::join_all;
        
        let futures = self.relayer_urls
            .iter()
            .map(|relayer_url| self.submit_to_relayer(bundle, relayer_url));
        
        join_all(futures).await
    }

    async fn submit_to_relayer(&self, bundle: &Bundle, relayer_url: &str) -> BundleSubmission {
        let params = EthSendBundleParams::new(bundle, relayer_url);
        let request = JsonRpcRequest::new(params);

        let default_submission =
            |success, bundle_hash: Option<String>, error: Option<String>| BundleSubmission::new(
                bundle.target_block(),
                bundle_hash,
                relayer_url.to_string(),
                success,
                error,
            );

        match self
            .send_request::<EthSendBundleParams, EthSendBundleResponse>(&request, relayer_url)
            .await
        {
            Ok(res) => match (res.error, res.result) {
                (Some(err), _) => default_submission(false, None, Some(err.message)),
                (None, Some(result)) => default_submission(true, Some(result.bundle_hash), None),
                _ => default_submission(false, None, Some("Empty response".into())),
            },
            Err(e) => default_submission(false, None, Some(e.to_string())),
        }
    }

    async fn send_request<T: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        request: &JsonRpcRequest<T>,
        relayer_url: &str,
    ) -> Result<JsonRpcResponse<R>> {
        let request_body = serde_json::to_string(request)?;
        let signature = self.sign_request(&request_body).await?;

        let response = self
            .http_client
            .post(relayer_url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .header("X-Flashbots-Signature", signature)
            .body(request_body)
            .send()
            .await?;

        let response_text = response.text().await?;
        let json_response: JsonRpcResponse<R> = serde_json::from_str(&response_text)
            .map_err(|e| BundleError::InvalidRelayerResponse { 
                url: relayer_url.to_string(),
                message: format!("Failed to parse response: {}", e) 
            })?;

        Ok(json_response)
    }

    async fn sign_request(&self, request_body: &str) -> Result<String> {
        let hash = keccak256(request_body.as_bytes());
        let message = format!("0x{}", hex::encode(hash));

        let signature = self
            .identity_signer
            .sign_message(message.as_bytes())
            .await?;

        Ok(format!(
            "{}:0x{}",
            self.identity_signer.address(),
            signature
        ))
    }
}
