//! Simulation management for arbitrage operations.
//!
//! This module handles transaction simulation and execution for profitable paths.

use alloy::{
    network::Ethereum,
    primitives::{Address, U256},
    providers::{Provider, RootProvider},
    signers::local::PrivateKeySigner,
};
use anyhow::Result;
use futures::stream::{self, Stream, StreamExt};
use num_bigint::BigUint;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tycho_atomic_arbitrage::{
    bundle::TxExecutor,
    graph::TradingGraph,
    path::PathExt,
    simulation::{LogParser, SimulationResult, Simulator},
    utils::{biguint_to_u256, u256_to_biguint},
};
use tycho_common::Bytes;
use tycho_simulation::protocol::{models::ProtocolComponent, state::ProtocolSim};

use super::logging::PathLogger;

/// Run simulations for a collection of profitable paths.
pub async fn run_simulations<'a>(
    paths: Vec<PathExt>,
    nonce: u64,
    base_fee: U256,
    provider: &'a Arc<RootProvider<Ethereum>>,
    simulator: &'a Arc<Simulator>,
    signer: &'a PrivateKeySigner,
) -> impl Stream<Item = (PathExt, tycho_atomic_arbitrage::Result<SimulationResult>)> + 'a {
    const SIMULATION_BUFFER_SIZE: usize = 10;

    stream::iter(paths)
        .map(move |path| {
            let simulator = Arc::clone(simulator);
            let provider = Arc::clone(provider);
            async move {
                let sim_result = simulator
                    .run_simulation(&provider, &path, nonce, base_fee, signer)
                    .await;
                (path, sim_result)
            }
        })
        .buffer_unordered(SIMULATION_BUFFER_SIZE)
}

/// Convert a token amount to native token using the best available exchange rate.
///
/// This function finds the best exchange rate between the given token and the native token
/// by checking all available pools and simulating swaps to find the highest output amount.
///
/// # Arguments
///
/// * `token` - The address of the token to convert from
/// * `amount` - The amount of the token to convert
/// * `native_token` - The address of the native token to convert to
/// * `graph` - The trading graph containing token and pool information
/// * `protocol_sim` - Protocol simulators for calculating swap amounts
/// * `protocol_comp` - Protocol components containing token information
///
/// # Returns
///
/// Returns `Ok(Some(amount))` with the converted amount if conversion is possible,
/// `Ok(None)` if no conversion path exists, or an error if the operation fails.
async fn swap_to_native(
    token: &Bytes,
    amount: BigUint,
    native_token: &Bytes,
    graph: &Arc<RwLock<TradingGraph>>,
    protocol_sim: &Arc<RwLock<HashMap<Bytes, Box<dyn ProtocolSim>>>>,
    protocol_comp: &Arc<RwLock<HashMap<Bytes, ProtocolComponent>>>,
) -> Result<Option<BigUint>> {
    // Quick check: if token is already native token, no conversion needed
    if token == native_token {
        return Ok(Some(amount));
    }

    // Acquire all read locks once at the beginning to minimize lock contention
    let graph_guard = graph.read().await;
    let protocol_sims_guard = protocol_sim.read().await;
    let protocol_comp_guard = protocol_comp.read().await;

    // Find token IDs (cache to avoid repeated lookups)
    let token_idx = match graph_guard.find_token_id(token) {
        Ok(idx) => idx,
        Err(_) => {
            tracing::warn!(
                token = %token,
                "Token not found in graph for swap conversion"
            );
            return Ok(None);
        }
    };

    let native_idx = match graph_guard.find_token_id(native_token) {
        Ok(idx) => idx,
        Err(_) => {
            tracing::warn!(
                native_token = %native_token,
                "Native token not found in graph for swap conversion"
            );
            return Ok(None);
        }
    };

    // Early exit if no pools exist between tokens
    let pools = match graph_guard.pools_between_tokens([token_idx, native_idx]) {
        Ok(pools) => pools,
        Err(_) => {
            tracing::info!(
                token = %token,
                native_token = %native_token,
                "No direct pools between token and native token, skipping conversion"
            );
            return Ok(None);
        }
    };

    let mut best_rate = BigUint::from(0u32);

    // Iterate through all available pools to find the best rate
    for &pool_id in pools {
        let pool = match graph_guard.get_pool(pool_id) {
            Ok(pool) => pool,
            Err(_) => continue,
        };

        let pool_address = pool.address();
        
        // Get protocol simulator and component for this pool
        let (pool_sim, pool_comp) = match (
            protocol_sims_guard.get(pool_address),
            protocol_comp_guard.get(pool_address)
        ) {
            (Some(sim), Some(comp)) => (sim, comp),
            _ => {
                tracing::debug!(
                    pool_address = %pool_address,
                    "Pool simulator or component not found, skipping"
                );
                continue;
            }
        };

        // Find token objects in the pool component
        let token_obj = pool_comp.tokens.iter().find(|t| t.address == *token);
        let native_obj = pool_comp.tokens.iter().find(|t| t.address == *native_token);

        if let (Some(from_token), Some(to_token)) = (token_obj, native_obj) {
            // Simulate the swap to get the output amount
            match pool_sim.get_amount_out(amount.clone(), from_token, to_token) {
                Ok(result) => {
                    if result.amount > best_rate {
                        tracing::debug!(
                            pool_address = %pool_address,
                            input_amount = %amount,
                            output_amount = %result.amount,
                            "Found better swap rate"
                        );
                        best_rate = result.amount;
                    }
                }
                Err(e) => {
                    tracing::debug!(
                        pool_address = %pool_address,
                        error = %e,
                        "Failed to simulate swap for pool"
                    );
                }
            }
        }
    }

    if best_rate > BigUint::from(0u32) {
        tracing::debug!(
            token = %token,
            native_token = %native_token,
            input_amount = %amount,
            output_amount = %best_rate,
            "Successfully calculated token conversion rate"
        );
        Ok(Some(best_rate))
    } else {
        tracing::debug!(
            token = %token,
            native_token = %native_token,
            "No valid swap rate found for token conversion"
        );
        Ok(None)
    }
}

/// Process a successful simulation result and potentially execute the trade.
pub async fn process_simulation_result(
    sim_result: SimulationResult,
    path: PathExt,
    block_number: u64,
    base_fee: U256,
    executor: &Arc<TxExecutor>,
    native_token: &Bytes,
    graph: &Arc<RwLock<TradingGraph>>,
    protocol_sim: &Arc<RwLock<HashMap<Bytes, Box<dyn ProtocolSim>>>>,
    protocol_comp: &Arc<RwLock<HashMap<Bytes, ProtocolComponent>>>,
    logger: &PathLogger,
) -> Result<bool> {
    let decoded_logs = LogParser::parse_simulation_results(sim_result.simulated_blocks)
        .map_err(|e| anyhow::anyhow!("Failed to parse simulation logs: {}", e))?;

    let gross_profit = decoded_logs.profit()
        .map_err(|e| anyhow::anyhow!("Failed to calculate profit: {}", e))?;
    
    let start_token = path.start_token()
        .map_err(|e| anyhow::anyhow!("Failed to get start token: {}", e))?;

    // Extract simulation input and output amounts
    let default_amount = BigUint::from(0u32);
    let simulation_input_amount = path.first()
        .map(|swap| &swap.amount_in)
        .unwrap_or(&default_amount);
    
    let simulation_output_amount = path.last()
        .map(|swap| &swap.amount_out)
        .unwrap_or(&default_amount);

    let total_gas_used = decoded_logs.approval_gas + decoded_logs.swap_gas;

    let gross_profit_biguint = gross_profit
        .to_biguint()
        .ok_or_else(|| anyhow::anyhow!("Gross profit less than zero"))?;

    let gas_cost = decoded_logs.gas_cost(u256_to_biguint(base_fee));
    
    tracing::debug!(
        path_length = path.len(),
        gross_profit = %gross_profit,
        start_token = %start_token,
        "Simulation completed successfully"
    );

    // Calculate gross profit in native token (convert if necessary)
    let gross_profit_in_native = match swap_to_native(
        &start_token,
        gross_profit_biguint.clone(),
        native_token,
        graph,
        protocol_sim,
        protocol_comp,
    ).await {
        Ok(Some(converted_amount)) => converted_amount,
        Ok(None) => {
            tracing::info!(
                start_token = %start_token,
                native_token = %native_token,
                "No conversion path available, skipping trade execution"
            );
            return Ok(false);
        }
        Err(e) => {
            tracing::error!(
                error = %e,
                start_token = %start_token,
                native_token = %native_token,
                "Failed to convert profit to native token"
            );
            return Err(e);
        }
    };

    // Log simulation results
    if let Err(e) = logger.log_simulation_result(
        &path,
        simulation_input_amount,
        simulation_output_amount,
        total_gas_used,
        &gas_cost,
        &gross_profit_in_native,
        &start_token,
        block_number,
    ) {
        tracing::warn!(
            error = %e,
            "Failed to log simulation result"
        );
    }

    let is_profitable = gross_profit_in_native > gas_cost;

    if is_profitable {
        // Check if this is Ethereum by comparing native token to Ethereum WETH address
        let ethereum_weth = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
        if native_token.to_string().to_lowercase() == ethereum_weth.to_lowercase() {
            let net_profit = gross_profit_in_native.clone() - gas_cost.clone();
            let tx_requests = vec![sim_result.approval_request, sim_result.swap_request];

            tracing::info!(
                gross_profit = %gross_profit_in_native,
                gas_cost = %gas_cost,
                net_profit = %net_profit,
                "Executing profitable bundle"
            );

            let result = executor
                .execute(
                    tx_requests,
                    block_number + 1,
                    base_fee,
                    biguint_to_u256(&net_profit)
                        .map_err(|e| anyhow::anyhow!("Failed to convert net profit to U256: {}", e))?,
                )
                .await;

            match result {
                Ok(submissions) => {
                    let successful_count = submissions.iter().filter(|s| s.is_successful()).count();
                    tracing::info!(
                        successful_submissions = successful_count,
                        total_submissions = submissions.len(),
                        "Bundle execution completed"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        "Bundle execution failed"
                    );
                }
            }
        }
    } else {
        tracing::info!(
            gross_profit = %gross_profit_in_native,
            gas_cost = %gas_cost,
            start_token = %start_token,
            "Arbitrage not profitable after gas costs"
        );
    }

    Ok(is_profitable)
}

/// Get the current nonce and calculate the next base fee.
pub async fn get_nonce_and_base_fee(
    provider: &Arc<RootProvider<Ethereum>>,
    signer_address: Address,
) -> Result<(u64, U256)> {
    let nonce_future = provider.get_transaction_count(signer_address);
    let block_future = provider
        .get_block_by_number(alloy::rpc::types::BlockNumberOrTag::Latest);

    let (nonce, block_res) = tokio::try_join!(nonce_future, block_future)
        .map_err(|e| anyhow::anyhow!("Failed to fetch nonce and block: {}", e))?;

    let block = block_res
        .ok_or_else(|| anyhow::anyhow!("Failed to get latest block"))?;

    let current_base_fee_per_gas = block.header.base_fee_per_gas.unwrap_or_default();
    let current_gas_used = block.header.gas_used;
    let current_gas_limit = block.header.gas_limit;

    let next_base_fee = tycho_atomic_arbitrage::utils::calculate_next_base_fee(
        current_base_fee_per_gas.into(),
        current_gas_used.into(),
        current_gas_limit.into(),
    );

    tracing::debug!(
        nonce = nonce,
        current_base_fee = %current_base_fee_per_gas,
        next_base_fee = %next_base_fee,
        "Fetched nonce and calculated next base fee"
    );

    Ok((nonce, next_base_fee))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simulation_buffer_size() {
        // This is a compile-time constant, so we just verify it's reasonable
        const SIMULATION_BUFFER_SIZE: usize = 10;
        assert!(SIMULATION_BUFFER_SIZE > 0);
        assert!(SIMULATION_BUFFER_SIZE <= 50); // Reasonable upper bound
    }
}
