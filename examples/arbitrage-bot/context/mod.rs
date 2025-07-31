//! Context management for atomic arbitrage operations.
//!
//! This module provides the main context structure and orchestrates
//! the various components needed for atomic arbitrage trading.

pub mod arbitrage;
pub mod balance;
pub mod components;
pub mod logging;
pub mod optimization;
pub mod optimizers;
pub mod simulation;

use crate::cli::Args;
use alloy::{
    providers::RootProvider,
    signers::local::PrivateKeySigner,
};
use std::{collections::HashMap, str::FromStr, sync::Arc};
use tycho_atomic_arbitrage::{
    bundle::TxExecutor,
    config::ArbitrageConfig,
    builders::SimulatorBuilder,
    errors::Result,
    graph::TradingGraph,
};
use tycho_common::Bytes;
use tycho_simulation::protocol::{
    models::{BlockUpdate, ProtocolComponent},
    state::ProtocolSim,
};

use components::{
    ArbitrageParams, ExecutionContext, MarketContext, MarketDataManager,
    PathFinder, SearchParams, TradeExecutor,
};
use logging::{PathLogger, RunConfiguration};

/// Main arbitrage context using component-based architecture.
///
/// This context orchestrates the various components needed for arbitrage trading
/// while maintaining clear separation of concerns.
pub struct Context {
    market_data: MarketDataManager,
    path_finder: PathFinder,
    trade_executor: TradeExecutor,
    params: ArbitrageParams,
    logger: PathLogger,
}

impl Context {
    pub fn new(args: Args) -> Result<Self> {
        let native_token = args.native_token()?;
        let source_tokens = args.start_tokens()?;

        let provider = Arc::new(RootProvider::new_http(
            args.rpc_url.parse()
                .map_err(|e| anyhow::anyhow!("Invalid RPC URL: {}", e))?,
        ));

        // Create configuration directly from the args
        let config = ArbitrageConfig::from_env(&args.chain)?;

        let simulator = SimulatorBuilder::from_config(&config)
            .build();

        let executor = TxExecutor::from_config(config)?;

        let signer = args.executor_private_key.parse::<PrivateKeySigner>()
            .map_err(|e| anyhow::anyhow!("Invalid swapper private key: {}", e))?;

        let optimization_tolerances = source_tokens
            .iter()
            .cloned()
            .zip(args.optimization_tolerances.iter().cloned())
            .collect();

        // Create components
        let market_data = MarketDataManager::new();
        let path_finder = PathFinder::new(source_tokens, optimization_tolerances);
        let trade_executor = TradeExecutor::new(simulator, executor, provider, signer);
        let params = ArbitrageParams::new(native_token.clone(), args.min_profit_bps);

        // Initialize logger with default output directory
        let logger = PathLogger::new("./arbitrage_logs")
            .map_err(|e| anyhow::anyhow!("Failed to initialize logger: {}", e))?;

        // Create and log the run configuration
        let run_config = RunConfiguration {
            timestamp: chrono::Utc::now(),
            chain: args.chain.clone(),
            rpc_url_masked: RunConfiguration::mask_url(&args.rpc_url),
            has_tycho_api_key: !args.tycho_api_key.is_empty(),
            start_tokens: args.start_tokens.clone(),
            start_token_addresses: path_finder.source_tokens.iter()
                .map(|token| token.to_string())
                .collect(),
            optimization_tolerances: args.optimization_tolerances.clone(),
            has_executor_private_key: !args.executor_private_key.is_empty(),
            tvl_threshold: args.tvl_threshold,
            min_profit_bps: args.min_profit_bps,
            slippage_bps: args.slippage_bps,
            has_flashbots_identity: args.flashbots_identity.is_some(),
            bribe_percentage: args.bribe_percentage,
            native_token_address: native_token.to_string(),
            tycho_url: args.tycho_url().unwrap_or_else(|_| "unknown".to_string()),
        };

        // Log the configuration to config.json
        if let Err(e) = logger.log_config(run_config) {
            tracing::warn!(
                error = %e,
                "Failed to log run configuration"
            );
        }

        tracing::info!(
            chain = args.chain,
            native_token = %native_token,
            source_tokens_count = path_finder.source_tokens.len(),
            min_profit_bps = args.min_profit_bps,
            "Context initialized successfully"
        );

        Ok(Self {
            market_data,
            path_finder,
            trade_executor,
            params,
            logger,
        })
    }

    pub async fn apply(&mut self, update: BlockUpdate) -> Result<Vec<Bytes>> {
        // Update balances
        balance::update_source_balances(
            &self.path_finder,
            &self.trade_executor.provider,
            self.trade_executor.signer.address(),
        ).await?;

        // Update block number
        self.market_data.update_block_number(update.block_number).await;

        // Handle market data updates
        self.handle_removed_pairs(&update.removed_pairs).await;
        self.handle_new_pairs(&update.new_pairs).await;
        self.handle_states(&update.states).await
    }

    pub async fn search(&self, updated_pools: Vec<Bytes>) -> Result<()> {
        let block_number = self.market_data.get_block_number().await;
        let search_params = SearchParams::new(updated_pools, block_number);
        let market_context = MarketContext::new(&self.market_data, &self.path_finder);
        let execution_context = ExecutionContext::new(&self.trade_executor, &self.params);

        arbitrage::execute_arbitrage_search(
            search_params,
            market_context,
            execution_context,
            &self.logger,
        ).await
    }

    async fn handle_states(
        &mut self,
        states: &HashMap<String, Box<dyn ProtocolSim>>,
    ) -> Result<Vec<Bytes>> {
        if states.is_empty() {
            return Ok(Vec::new());
        }

        let mut write_guard = self.market_data.protocol_sim.write().await;
        let mut updated_pools = Vec::new();

        tracing::info!(state_updates = states.len(), "Processing state updates");
        
        for (key, sim) in states {
            match Bytes::from_str(key) {
                Ok(pool) => {
                    write_guard.insert(pool.clone(), sim.clone());
                    updated_pools.push(pool);
                }
                Err(e) => {
                    tracing::warn!(
                        pool_key = key,
                        error = %e,
                        "Failed to parse pool address from state update"
                    );
                }
            }
        }

        tracing::debug!(
            updated_pools_count = updated_pools.len(),
            "State updates processed successfully"
        );

        Ok(updated_pools)
    }

    async fn handle_new_pairs(&mut self, new_pairs: &HashMap<String, ProtocolComponent>) {
        if new_pairs.is_empty() {
            return;
        }

        let mut guard_comp = self.market_data.protocol_comp.write().await;
        let mut guard_graph = self.market_data.graph.write().await;
        let mut guard_paths = self.path_finder.paths.write().await;

        let mut new_node_idxs = Vec::new();
        let mut new_edge_idxs = Vec::new();

        tracing::info!(new_pairs_count = new_pairs.len(), "Processing new pairs");
        
        for (key, comp) in new_pairs {
            match Bytes::from_str(key) {
                Ok(pool_address) => {
                    guard_comp.insert(pool_address.clone(), comp.clone());
                    
                    match guard_graph.add_protocol_component(pool_address.clone(), comp.clone()) {
                        Ok(pool_infos) => {
                            for pool_info in &pool_infos {
                                new_node_idxs.extend(pool_info.token_ids);
                                new_edge_idxs.extend(pool_info.pool_ids);
                            }
                            
                            tracing::debug!(
                                pool_address = %pool_address,
                                protocol_system = %comp.protocol_system,
                                token_count = comp.tokens.len(),
                                pairs_created = pool_infos.len(),
                                "New pair added successfully"
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                pool_address = %pool_address,
                                error = %e,
                                "Failed to add protocol component to graph"
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        pool_key = key,
                        error = %e,
                        "Failed to parse pool address from new pair"
                    );
                }
            }
        }

        // Sort and deduplicate indices
        new_node_idxs.sort_unstable();
        new_node_idxs.dedup();
        new_edge_idxs.sort_unstable();
        new_edge_idxs.dedup();

        if !new_node_idxs.is_empty() && !new_edge_idxs.is_empty() {
            let pool_paths_count_before = guard_paths.pool_paths.len();
            let token_paths_count_before = guard_paths.token_paths.len();
            
            guard_paths.discover_paths(
                &guard_graph,
                new_node_idxs[0],
                new_node_idxs.len(),
                new_edge_idxs[0],
                new_edge_idxs.len(),
            );
            
            let pool_paths_count_after = guard_paths.pool_paths.len();
            let token_paths_count_after = guard_paths.token_paths.len();
            let new_paths_count = pool_paths_count_after - pool_paths_count_before;
            
            // Debug log to show the breakdown between token paths and pool paths
            tracing::debug!(
                token_paths_added = token_paths_count_after - token_paths_count_before,
                pool_paths_added = new_paths_count,
                "Path discovery breakdown"
            );
            
            // Log newly created paths
            if new_paths_count > 0 {
                for path_idx in pool_paths_count_before..pool_paths_count_after {
                    // Extract pool addresses from the pool path
                    if let Some(pool_path) = guard_paths.pool_paths.get(path_idx) {
                        let mut pools = Vec::new();
                        
                        // Get pools from the pool path
                        for &pool_idx in pool_path {
                            if let Ok(pool) = guard_graph.get_pool(pool_idx) {
                                pools.push(pool.address().clone());
                            }
                        }
                        
                        // Derive tokens from the pool sequence to ensure consistency
                        let tokens = if let Ok(derived_tokens) = self.derive_tokens_from_pool_path(&guard_graph, pool_path) {
                            derived_tokens
                        } else {
                            tracing::warn!(
                                path_index = path_idx,
                                pool_count = pools.len(),
                                "Failed to derive tokens from pool path"
                            );
                            continue;
                        };
                        
                        // Validate path consistency before logging
                        if let Err(e) = tycho_atomic_arbitrage::path::PathValidator::validate_path_consistency(&pools, &tokens) {
                            tracing::warn!(
                                path_index = path_idx,
                                pool_count = pools.len(),
                                token_count = tokens.len(),
                                error = %e,
                                "Skipping inconsistent path"
                            );
                            continue;
                        }

                        if let Err(e) = self.logger.log_path(&pools, &tokens) {
                            tracing::warn!(
                                error = %e,
                                path_index = path_idx,
                                pool_count = pools.len(),
                                token_count = tokens.len(),
                                "Failed to log newly created path"
                            );
                        }
                    }
                }
                
                tracing::debug!(
                    new_paths_logged = new_paths_count,
                    "Logged newly created paths to CSV"
                );
            }
            
            tracing::info!(
                new_nodes = new_node_idxs.len(),
                new_edges = new_edge_idxs.len(),
                new_paths = new_paths_count,
                "Paths added successfully for new pairs"
            );
        } else {
            tracing::warn!("Cannot add paths - empty node or edge vectors");
        }
    }

    /// Derive the token sequence from a pool path to ensure consistency.
    ///
    /// For an arbitrage path, we need to trace through the pools to determine
    /// the correct sequence of tokens. This ensures that the logged tokens
    /// match exactly with the pools used.
    fn derive_tokens_from_pool_path(
        &self,
        graph: &TradingGraph,
        pool_path: &[usize],
    ) -> Result<Vec<Bytes>> {
        if pool_path.is_empty() {
            return Ok(Vec::new());
        }

        let mut tokens = Vec::new();
        
        // For the first pool, we need to determine which token is the input
        // We'll use the first source token that appears in the first pool
        let first_pool = graph.get_pool(pool_path[0])
            .map_err(|e| anyhow::anyhow!("Failed to get first pool: {}", e))?;
        
        let first_pool_tokens = first_pool.tokens();
        let mut current_token_idx = None;
        
        // Find a source token that's in the first pool
        for source_token in &self.path_finder.source_tokens {
            for token_idx in first_pool_tokens {
                if let Ok(token) = graph.get_token(token_idx) {
                    if token.address() == source_token {
                        current_token_idx = Some(token_idx);
                        tokens.push(source_token.clone());
                        break;
                    }
                }
            }
            if current_token_idx.is_some() {
                break;
            }
        }
        
        if current_token_idx.is_none() {
            return Err(anyhow::anyhow!("No source token found in first pool").into());
        }
        
        let mut current_token_idx = current_token_idx.unwrap();
        
        // Trace through each pool to find the output token
        for &pool_idx in pool_path {
            let pool = graph.get_pool(pool_idx)
                .map_err(|e| anyhow::anyhow!("Failed to get pool {}: {}", pool_idx, e))?;
            
            let pool_tokens = pool.tokens();
            
            // Find the other token in this pool (the output token)
            let mut next_token_idx = None;
            for token_idx in pool_tokens {
                if token_idx != current_token_idx {
                    next_token_idx = Some(token_idx);
                    break;
                }
            }
            
            if let Some(next_idx) = next_token_idx {
                if let Ok(next_token) = graph.get_token(next_idx) {
                    tokens.push(next_token.address().clone());
                    current_token_idx = next_idx;
                } else {
                    return Err(anyhow::anyhow!("Failed to get token {}", next_idx).into());
                }
            } else {
                return Err(anyhow::anyhow!("Could not find output token for pool {}", pool_idx).into());
            }
        }
        
        Ok(tokens)
    }
    

    async fn handle_removed_pairs(&mut self, removed_pairs: &HashMap<String, ProtocolComponent>) {
        if removed_pairs.is_empty() {
            return;
        }

        let mut guard_sim = self.market_data.protocol_sim.write().await;
        let mut guard_comp = self.market_data.protocol_comp.write().await;

        tracing::info!(removed_pairs_count = removed_pairs.len(), "Processing removed pairs");

        for (key, _) in removed_pairs {
            match Bytes::from_str(key) {
                Ok(pool_address) => {
                    guard_sim.remove(&pool_address);
                    guard_comp.remove(&pool_address);
                    
                    tracing::debug!(
                        pool_address = %pool_address,
                        "Pair removed successfully"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        pool_key = key,
                        error = %e,
                        "Failed to parse pool address from removed pair"
                    );
                }
            }
        }
    }
}
