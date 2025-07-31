//! Main arbitrage execution logic.
//!
//! This module orchestrates the complete arbitrage search and execution process,
//! coordinating between path optimization, simulation, and trade execution.

use futures::StreamExt;
use tycho_atomic_arbitrage::errors::Result;

use super::{
    components::{ExecutionContext, MarketContext, SearchParams},
    logging::{BlockSummary, PathLogger},
    optimization, simulation,
};

/// Execute the complete arbitrage search process.
///
/// This function coordinates the entire arbitrage workflow:
/// 1. Filter and optimize paths for profitability
/// 2. Get current nonce and base fee
/// 3. Run simulations for profitable paths
/// 4. Process simulation results and execute profitable trades
pub async fn execute_arbitrage_search(
    search_params: SearchParams,
    market_context: MarketContext<'_>,
    execution_context: ExecutionContext<'_>,
    logger: &PathLogger,
) -> Result<()> {
    tracing::info!(
        block_number = search_params.block_number,
        updated_pools_count = search_params.updated_pools.len(),
        "Starting arbitrage search"
    );

    // Step 1: Filter and optimize paths
    let (profitable_paths, initial_paths, candidate_paths) = optimization::filter_and_optimize_paths(
        search_params.updated_pools,
        &market_context.path_finder.paths,
        &market_context.market_data.graph,
        &market_context.market_data.protocol_sim,
        &market_context.market_data.protocol_comp,
        &market_context.path_finder.source_balances,
        &market_context.path_finder.optimization_tolerances,
        execution_context.params.min_profit_bps,
        search_params.block_number,
        logger,
    ).await?;

    if profitable_paths.is_empty() {
        // Log block summary even if no profitable paths found
        let block_summary = BlockSummary {
            block_number: search_params.block_number,
            initial_paths,
            candidate_paths,
            optimised_profitable_paths: 0,
            successful_simulations: 0,
            profitable_simulations: 0,
        };
        
        if let Err(e) = logger.log_block_summary(&block_summary) {
            tracing::warn!(
                error = %e,
                "Failed to log block summary"
            );
        }
        
        tracing::info!("No profitable paths found");
        return Ok(());
    }

    let profitable_paths_count = profitable_paths.len();
    
    tracing::info!(
        profitable_paths_count = profitable_paths_count,
        "Found profitable paths, proceeding with simulations"
    );

    // Step 2: Get current nonce and base fee
    let (nonce, base_fee) = simulation::get_nonce_and_base_fee(
        &execution_context.trade_executor.provider,
        execution_context.trade_executor.signer.address(),
    ).await?;

    tracing::debug!(
        nonce = nonce,
        base_fee = %base_fee,
        "Retrieved nonce and base fee for simulations"
    );

    // Step 3: Run simulations
    let mut simulation_stream = simulation::run_simulations(
        profitable_paths,
        nonce,
        base_fee,
        &execution_context.trade_executor.provider,
        &execution_context.trade_executor.simulator,
        &execution_context.trade_executor.signer,
    ).await;

    let mut processed_count = 0;
    let mut successful_count = 0;
    let mut failed_count = 0;
    let mut profitable_count = 0;

    // Step 4: Process simulation results
    while let Some((path, sim_result)) = simulation_stream.next().await {
        processed_count += 1;
        
        match sim_result {
            Ok(simulation_result) => {
                match simulation::process_simulation_result(
                    simulation_result,
                    path,
                    search_params.block_number,
                    base_fee,
                    &execution_context.trade_executor.executor,
                    &execution_context.params.native_token,
                    &market_context.market_data.graph,
                    &market_context.market_data.protocol_sim,
                    &market_context.market_data.protocol_comp,
                    logger,
                ).await {
                    Ok(was_profitable) => {
                        successful_count += 1;
                        if was_profitable {
                            profitable_count += 1;
                        }
                        tracing::debug!("Simulation result processed successfully");
                    }
                    Err(e) => {
                        failed_count += 1;
                        tracing::error!(
                            error = %e,
                            "Failed to process simulation result"
                        );
                    }
                }
            }
            Err(e) => {
                failed_count += 1;
                tracing::error!(
                    error = %e,
                    "Simulation failed for path"
                );
            }
        }
    }

    // Log block summary with all collected statistics
    let block_summary = BlockSummary {
        block_number: search_params.block_number,
        initial_paths,
        candidate_paths,
        optimised_profitable_paths: profitable_paths_count,
        successful_simulations: successful_count,
        profitable_simulations: profitable_count,
    };
    
    if let Err(e) = logger.log_block_summary(&block_summary) {
        tracing::warn!(
            error = %e,
            "Failed to log block summary"
        );
    }

    tracing::info!(
        block_number = search_params.block_number,
        processed_simulations = processed_count,
        successful_simulations = successful_count,
        failed_simulations = failed_count,
        profitable_simulations = profitable_count,
        success_rate = if processed_count > 0 {
            format!("{:.1}%", (successful_count as f64 / processed_count as f64) * 100.0)
        } else {
            "N/A".to_string()
        },
        "Arbitrage search completed"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_success_rate_calculation() {
        let successful = 8;
        let total = 10;
        let rate = (successful as f64 / total as f64) * 100.0;
        assert_eq!(rate, 80.0);
    }

    #[test]
    fn test_success_rate_zero_total() {
        let successful = 0;
        let total = 0;
        let rate_str = if total > 0 {
            format!("{:.1}%", (successful as f64 / total as f64) * 100.0)
        } else {
            "N/A".to_string()
        };
        assert_eq!(rate_str, "N/A");
    }
}
