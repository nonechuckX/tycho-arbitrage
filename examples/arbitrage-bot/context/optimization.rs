//! Path optimization for arbitrage operations.
//!
//! This module handles the filtering and optimization of trading paths
//! to find profitable arbitrage opportunities.

use num_bigint::BigUint;
use num_traits::ToPrimitive;
use rayon::prelude::*;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tycho_atomic_arbitrage::{
    errors::Result,
    graph::TradingGraph,
    path::{Path, PathExt, PathRepository, PathOptimizer},
};
use tycho_common::Bytes;
use tycho_simulation::protocol::{models::ProtocolComponent, state::ProtocolSim};

use super::{logging::PathLogger, optimizers::TernarySearchOptimizer};

/// Returns the lower bound for optimization (BigUint from 1u32)
fn optimizer_lower_bound() -> BigUint {
    1u32.into()
}

/// Filter and optimize paths for arbitrage opportunities.
pub async fn filter_and_optimize_paths(
    updated_pools: Vec<Bytes>,
    paths: &Arc<RwLock<PathRepository>>,
    graph: &Arc<RwLock<TradingGraph>>,
    protocol_sim: &Arc<RwLock<HashMap<Bytes, Box<dyn ProtocolSim>>>>,
    protocol_comp: &Arc<RwLock<HashMap<Bytes, ProtocolComponent>>>,
    source_balances: &Arc<RwLock<HashMap<Bytes, BigUint>>>,
    optimization_tolerances: &HashMap<Bytes, f64>,
    min_profit_bps: u64,
    block_number: u64,
    logger: &PathLogger,
) -> Result<(Vec<PathExt>, usize, usize)> {
    tracing::debug!(
        updated_pools_count = updated_pools.len(),
        "Starting path filtering and optimization"
    );

    let mut paths = get_paths_of_pools(updated_pools, paths, graph, protocol_sim, protocol_comp).await?;
    let initial_path_count = paths.len();
    
    tracing::debug!(
        initial_paths = initial_path_count,
        "Retrieved paths from updated pools"
    );

    // Filter paths by spot price product > threshold
    let threshold = 1.0 + 0.01 * (min_profit_bps as f64 / 100.0);
    paths.retain(|path| {
        match path.spot_price_product() {
            Ok(product) => product > threshold,
            Err(e) => {
                tracing::debug!(
                    error = %e,
                    "Failed to calculate spot price product, filtering out path"
                );
                false
            }
        }
    });
    
    let filtered_path_count = paths.len();
    
    tracing::info!(
        initial_paths = initial_path_count,
        filtered_paths = filtered_path_count,
        filtered_out = initial_path_count - filtered_path_count,
        threshold = threshold,
        "Filtered paths by spot price product"
    );

    let balances = source_balances.read().await;

    let path_exts: Vec<_> = paths
        .par_iter()
        .filter_map(|path| {
            optimize_single_path(path, &balances, optimization_tolerances)
        })
        .filter(|path_ext| {
            match path_ext.is_profitable() {
                Ok(is_profitable) => {
                    if !is_profitable {
                        if let Ok(start_token) = path_ext.start_token() {
                            tracing::debug!(
                                start_token = %start_token,
                                "Filtering out unprofitable path"
                            );
                        }
                    }
                    is_profitable
                }
                Err(e) => {
                    tracing::debug!(
                        error = %e,
                        "Failed to check profitability, filtering out path"
                    );
                    false
                }
            }
        })
        .collect();

    let profitable_path_count = path_exts.len();
    
    // Log filtered paths to CSV
    for path_ext in &path_exts {
        // Calculate spot price product from the PathExt directly
        let spot_price_product = calculate_spot_price_product_from_path_ext(path_ext);
        
        if let Err(e) = logger.log_filtered_path(path_ext, spot_price_product, block_number) {
            tracing::warn!(
                error = %e,
                "Failed to log filtered path"
            );
        }
    }
    
    tracing::info!(
        initial_paths = initial_path_count,
        filtered_paths = filtered_path_count,
        profitable_paths = profitable_path_count,
        optimization_success_rate = if filtered_path_count > 0 {
            format!("{:.1}%", (profitable_path_count as f64 / filtered_path_count as f64) * 100.0)
        } else {
            "N/A".to_string()
        },
        "Path filtering and optimization completed"
    );

    Ok((path_exts, initial_path_count, filtered_path_count))
}

/// Optimize a single path using the provided balances and tolerances.
fn optimize_single_path(
    path: &Path,
    balances: &HashMap<Bytes, BigUint>,
    optimization_tolerances: &HashMap<Bytes, f64>,
) -> Option<PathExt> {
    let start_token = path.start_token().ok()?;
    let upper_bound = balances.get(&start_token)?.clone();
    let tolerance_percentage = *optimization_tolerances.get(&start_token)?;
    
    // Calculate tolerance as absolute value
    let tolerance_f64 = upper_bound.to_string().parse::<f64>().unwrap_or(0.0) * tolerance_percentage / 100.0;

    tracing::debug!(
        start_token = %start_token,
        upper_bound = %upper_bound,
        tolerance_percentage = tolerance_percentage,
        "Optimizing path with parameters"
    );

    // Create optimizer with appropriate search range and tolerance
    let optimizer = TernarySearchOptimizer::new()
        .with_search_range(optimizer_lower_bound(), upper_bound)
        .with_tolerance(tolerance_f64.max(1.0)) // Ensure minimum tolerance of 1.0
        .with_max_iterations(100);
    
    match optimizer.optimize_and_execute(path) {
        Ok((optimization_result, path_ext)) => {
            tracing::debug!(
                start_token = %start_token,
                optimal_amount = %optimization_result.optimal_amount,
                expected_profit = %optimization_result.expected_profit,
                iterations = optimization_result.iterations,
                converged = optimization_result.converged,
                "Path optimization completed"
            );
            Some(path_ext)
        }
        Err(e) => {
            tracing::debug!(
                start_token = %start_token,
                error = %e,
                "Path optimization failed"
            );
            None
        }
    }
}

/// Calculate spot price product from a PathExt by using the swap simulators.
fn calculate_spot_price_product_from_path_ext(path_ext: &PathExt) -> f64 {
    let mut product = 1.0;
    
    for swap in path_ext.iter() {
        match swap.pool_sim.spot_price(swap.token_in(), swap.token_out()) {
            Ok(price) => product *= price,
            Err(_) => {
                // If we can't get spot price, estimate from amounts
                let price_estimate = swap.amount_out.clone().to_f64().unwrap_or(1.0) 
                    / swap.amount_in.clone().to_f64().unwrap_or(1.0);
                product *= price_estimate;
            }
        }
    }
    
    product
}

/// Get paths that involve the specified pools.
async fn get_paths_of_pools(
    updated_pools: Vec<Bytes>,
    paths: &Arc<RwLock<PathRepository>>,
    graph: &Arc<RwLock<TradingGraph>>,
    protocol_sim: &Arc<RwLock<HashMap<Bytes, Box<dyn ProtocolSim>>>>,
    protocol_comp: &Arc<RwLock<HashMap<Bytes, ProtocolComponent>>>,
) -> Result<Vec<Path>> {
    let graph_guard = graph.read().await;
    let paths_repo = paths.read().await;
    let protocol_sim_guard = protocol_sim.read().await;
    let protocol_comp_guard = protocol_comp.read().await;
    
    let path_idxs = paths_repo.get_path_indices_for_pools(&updated_pools)?;

    paths_repo.build_paths_from_indices(
        path_idxs,
        &graph_guard,
        &protocol_sim_guard,
        &protocol_comp_guard,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_optimizer_lower_bound() {
        assert_eq!(optimizer_lower_bound(), BigUint::from(1u32));
    }

    #[test]
    fn test_threshold_calculation() {
        let min_profit_bps = 100u64;
        let threshold = 1.0 + 0.01 * (min_profit_bps as f64 / 100.0);
        assert_eq!(threshold, 1.01);
    }
}
