//! Balance management for arbitrage operations.
//!
//! This module handles token balance queries and updates for the arbitrage bot.

use alloy::{
    network::Ethereum,
    primitives::{Address, U256},
    providers::{Provider, RootProvider},
};
use futures::stream::{self, StreamExt};
use num_bigint::BigUint;
use std::sync::Arc;
use tycho_atomic_arbitrage::{errors::Result, utils::u256_to_biguint};
use tycho_common::Bytes;

/// Update the source token balances for the given signer address.
pub async fn update_source_balances(
    path_finder: &super::components::PathFinder,
    provider: &Arc<RootProvider<Ethereum>>,
    signer_address: Address,
) -> Result<()> {
    let balance_futures = path_finder.source_tokens.iter().map(|token| {
        let provider = Arc::clone(provider);
        let token_address = Address::from_slice(token.as_ref());
        async move {
            let balance = get_token_balance(provider, token_address, signer_address).await?;
            Ok((token.clone(), balance))
        }
    });

    let results: Vec<Result<(Bytes, BigUint)>> = stream::iter(balance_futures)
        .buffer_unordered(10)
        .collect()
        .await;

    let mut balances = path_finder.source_balances.write().await;
    let mut successful_updates = 0;

    for result in results {
        match result {
            Ok((token, balance)) => {
                balances.insert(token.clone(), balance.clone());
                successful_updates += 1;
                
                tracing::debug!(
                    token = %token,
                    balance = %balance,
                    "Token balance updated"
                );
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    "Failed to update token balance"
                );
            }
        }
    }

    tracing::debug!(
        total_tokens = path_finder.source_tokens.len(),
        successful_updates = successful_updates,
        "Balance update completed"
    );

    if successful_updates == 0 {
        return Err(anyhow::anyhow!("Failed to update any token balances").into());
    }

    Ok(())
}

/// Get the balance of a specific token for a given owner address.
async fn get_token_balance(
    provider: Arc<RootProvider<Ethereum>>,
    token_address: Address,
    owner_address: Address,
) -> Result<BigUint> {
    // Construct the balanceOf(address) call data
    let mut call_data = vec![0x70, 0xa0, 0x82, 0x31]; // balanceOf(address) selector
    call_data.extend_from_slice(&[0u8; 12]); // Padding
    call_data.extend_from_slice(owner_address.as_slice());

    let tx = alloy::rpc::types::TransactionRequest {
        to: Some(alloy::primitives::TxKind::Call(token_address)),
        input: alloy::rpc::types::TransactionInput {
            input: Some(call_data.into()),
            data: None,
        },
        ..Default::default()
    };

    let result = provider
        .call(tx.into())
        .await
        .map_err(|e| anyhow::anyhow!("RPC call failed: {}", e))?;

    // Parse the result as a U256
    let balance_bytes = result.to_vec();
    let balance = if balance_bytes.len() >= 32 {
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&balance_bytes[balance_bytes.len() - 32..]);
        U256::from_be_bytes(bytes)
    } else {
        U256::ZERO
    };

    Ok(u256_to_biguint(balance))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[tokio::test]
    #[ignore] // Requires network access
    async fn test_get_token_balance() {
        let provider = Arc::new(RootProvider::new_http(
            "https://eth.llamarpc.com".parse().unwrap(),
        ));
        
        // USDC contract address
        let token_address = Address::from_str("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48").unwrap();
        // Vitalik's address
        let owner_address = Address::from_str("0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045").unwrap();
        
        let balance = get_token_balance(provider, token_address, owner_address).await;
        assert!(balance.is_ok());
    }
}
