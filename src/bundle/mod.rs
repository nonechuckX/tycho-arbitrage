//! Bundle management and transaction execution for atomic arbitrage.
//! 
//! This module provides the core bundle functionality:
//! - `Bundle`: A collection of transactions to be executed atomically
//! - `BundleSubmission`: Result of submitting a bundle to relayers
//! - `TxExecutor`: High-level interface for executing arbitrage transactions

pub mod relay;

// Re-export relay types for convenience
pub use relay::RelayClient;

use alloy::consensus::{SignableTransaction, TxEnvelope};
use alloy::eips::Encodable2718;
use alloy::network::TxSignerSync;
use alloy::primitives::U256;
use alloy::rpc::types::TransactionRequest;
use alloy::signers::local::PrivateKeySigner;
use crate::config::ArbitrageConfig;
use crate::errors::{BundleError, Result};
use std::sync::Arc;

/// A bundle submission result from a relayer.
#[derive(Debug, Clone)]
pub struct BundleSubmission {
    target_block: u64,
    bundle_hash: Option<String>,
    relayer_url: String,
    success: bool,
    error: Option<String>,
}

impl BundleSubmission {
    /// Create a new bundle submission result.
    pub fn new(
        target_block: u64,
        bundle_hash: Option<String>,
        relayer_url: String,
        success: bool,
        error: Option<String>,
    ) -> Self {
        Self {
            target_block,
            bundle_hash,
            relayer_url,
            success,
            error,
        }
    }

    /// Get the target block number for this submission.
    pub fn target_block(&self) -> u64 {
        self.target_block
    }

    /// Get the bundle hash if the submission was successful.
    pub fn bundle_hash(&self) -> Option<&str> {
        self.bundle_hash.as_deref()
    }

    /// Get the relayer URL this bundle was submitted to.
    pub fn relayer_url(&self) -> &str {
        &self.relayer_url
    }

    /// Check if the submission was successful.
    pub fn is_successful(&self) -> bool {
        self.success
    }

    /// Get the error message if the submission failed.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// A bundle of transactions to be executed atomically.
#[derive(Debug, Clone)]
pub struct Bundle {
    transactions: [String; 2],
    target_block: u64,
}

impl Bundle {
    /// Create a new bundle with the given transactions and target block.
    pub fn new(transactions: [String; 2], target_block: u64) -> Self {
        Self {
            transactions,
            target_block,
        }
    }

    /// Get the transactions in this bundle.
    pub fn transactions(&self) -> &[String; 2] {
        &self.transactions
    }

    /// Get the target block number for this bundle.
    pub fn target_block(&self) -> u64 {
        self.target_block
    }

    /// Get the number of transactions in this bundle.
    pub fn transaction_count(&self) -> usize {
        self.transactions.len()
    }
}

/// High-level transaction executor for arbitrage operations.
pub struct TxExecutor {
    relay_client: Arc<RelayClient>,
    config: ArbitrageConfig,
}

impl TxExecutor {
    /// Create a new TxExecutor from configuration.
    /// 
    /// # Arguments
    /// 
    /// * `config` - The arbitrage configuration containing security settings and relayer URLs
    pub fn from_config(config: ArbitrageConfig) -> Result<Self> {
        // Use the flashbots identity from config, or generate a random one for testing
        let identity_key = if let Some(identity) = config.flashbots_identity() {
            hex::encode(identity.credential().to_bytes())
        } else {
            // Generate a random identity for testing/development
            let random_identity = PrivateKeySigner::random();
            hex::encode(random_identity.credential().to_bytes())
        };

        let relay_client = Arc::new(RelayClient::from_config(&config, &identity_key)?);

        Ok(Self {
            relay_client,
            config,
        })
    }


    /// Update transaction requests with bribe and fee information.
    fn update_requests(
        &self,
        mut reqs: Vec<TransactionRequest>,
        base_fee: U256,
        profit: U256,
    ) -> [TransactionRequest; 2] {
        let bribe = profit * U256::from(self.config.bribe_percentage) / U256::from(100);
        
        // Update the swap request (second transaction) with bribe
        reqs[1].max_priority_fee_per_gas = Some(bribe.to());
        reqs[1].max_fee_per_gas = Some((base_fee + bribe).to());

        // Convert to array without cloning
        let mut iter = reqs.into_iter();
        [iter.next().unwrap(), iter.next().unwrap()]
    }

    /// Execute arbitrage transactions by submitting them as a bundle.
    /// 
    /// # Arguments
    /// 
    /// * `tx_requests` - The transaction requests to execute
    /// * `target_block` - The block number to target for execution
    /// * `base_fee` - The base fee for the target block
    /// * `profit_after_gas` - The expected profit after gas costs
    /// 
    /// # Returns
    /// 
    /// A vector of bundle submission results, one for each relayer.
    pub async fn execute(
        &self,
        tx_requests: Vec<TransactionRequest>,
        target_block: u64,
        base_fee: U256,
        profit_after_gas: U256,
    ) -> Result<Vec<BundleSubmission>> {
        tracing::info!(
            target_block = target_block,
            base_fee = %base_fee,
            profit_after_gas = %profit_after_gas,
            tx_count = tx_requests.len(),
            "Starting bundle execution"
        );

        let reqs = self.update_requests(tx_requests, base_fee, profit_after_gas);
        
        tracing::debug!(
            bribe_percentage = self.config.bribe_percentage,
            "Updated transaction requests with bribe information"
        );

        let transactions: [String; 2] = [
            format!("0x{}", hex::encode(self.sign_and_encode_transaction(reqs[0].clone())?)),
            format!("0x{}", hex::encode(self.sign_and_encode_transaction(reqs[1].clone())?)),
        ];

        tracing::debug!(
            tx_hashes = ?transactions.iter().map(|tx| &tx[..10]).collect::<Vec<_>>(),
            "Transactions signed and encoded"
        );

        let bundle = Bundle::new(transactions, target_block);
        let submission_results = self.relay_client.submit_bundle(&bundle).await;

        // Log submission results
        let successful_submissions = submission_results.iter().filter(|s| s.is_successful()).count();
        let total_submissions = submission_results.len();

        tracing::info!(
            target_block = target_block,
            successful_submissions = successful_submissions,
            total_submissions = total_submissions,
            success_rate = format!("{:.1}%", (successful_submissions as f64 / total_submissions as f64) * 100.0),
            "Bundle submission completed"
        );

        // Log individual submission details
        for submission in &submission_results {
            if submission.is_successful() {
                tracing::info!(
                    relayer_url = submission.relayer_url(),
                    bundle_hash = ?submission.bundle_hash(),
                    target_block = submission.target_block(),
                    "Bundle submitted successfully to relayer"
                );
            } else {
                tracing::warn!(
                    relayer_url = submission.relayer_url(),
                    error = ?submission.error(),
                    target_block = submission.target_block(),
                    "Bundle submission failed for relayer"
                );
            }
        }

        Ok(submission_results)
    }

    /// Sign and encode a transaction request.
    fn sign_and_encode_transaction(&self, tx_request: TransactionRequest) -> Result<Vec<u8>> {
        let mut typed_tx = tx_request
            .build_typed_tx()
            .map_err(|_| BundleError::TransactionSigningFailed { 
                reason: "Failed to build typed tx".to_string() 
            })?;

        let signature = self.config.executor_signer().sign_transaction_sync(&mut typed_tx)?;
        let signed_tx = typed_tx.into_signed(signature);
        let tx_envelope = TxEnvelope::from(signed_tx);
        let encoded_tx = tx_envelope.encoded_2718();

        Ok(encoded_tx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::consensus::transaction::SignerRecoverable;
    use alloy::consensus::TxEnvelope;
    use alloy::primitives::{Address, U256};
    use alloy::rlp::Decodable;
    use alloy::rpc::types::{TransactionInput, TransactionRequest};

    #[tokio::test]
    async fn test_sign_and_encode_transaction() {
        let config = ArbitrageConfig::for_testing("ethereum").unwrap();
        let executor = TxExecutor::from_config(config).unwrap();

        let tx_request = TransactionRequest {
            to: Some(alloy::primitives::TxKind::Call(Address::random())),
            value: Some(U256::from(10)),
            chain_id: Some(1),
            input: TransactionInput {
                input: None,
                data: None,
            },
            gas: Some(100_000),
            max_fee_per_gas: Some(1_000_000_000u128),
            max_priority_fee_per_gas: Some(1u128),
            nonce: Some(370),
            ..Default::default()
        };

        let result = executor.sign_and_encode_transaction(tx_request.clone());
        assert!(result.is_ok());

        let encoded_tx = result.unwrap();
        assert!(!encoded_tx.is_empty());

        let decoded_tx = TxEnvelope::decode(&mut encoded_tx.as_slice());
        assert!(decoded_tx.is_ok());

        let signed_tx = decoded_tx.unwrap();
        let recovered_signer = signed_tx.recover_signer().unwrap();
        assert_eq!(recovered_signer, executor.config.executor_signer().address());
    }
}
