//! Core components for the arbitrage system.
//!
//! This module defines focused components that each handle a specific aspect
//! of the arbitrage system, following single responsibility principle.

use alloy::{
    network::Ethereum,
    providers::RootProvider,
    signers::local::PrivateKeySigner,
};
use num_bigint::BigUint;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tycho_atomic_arbitrage::{
    bundle::TxExecutor,
    graph::TradingGraph,
    path::PathRepository,
    simulation::Simulator,
};
use tycho_common::Bytes;
use tycho_simulation::protocol::{models::ProtocolComponent, state::ProtocolSim};

/// Manages market data including protocol states, components, and trading graph.
#[derive(Debug)]
pub struct MarketDataManager {
    pub protocol_sim: Arc<RwLock<HashMap<Bytes, Box<dyn ProtocolSim>>>>,
    pub protocol_comp: Arc<RwLock<HashMap<Bytes, ProtocolComponent>>>,
    pub graph: Arc<RwLock<TradingGraph>>,
    pub block_number: Arc<RwLock<u64>>,
}

impl MarketDataManager {
    pub fn new() -> Self {
        Self {
            protocol_sim: Arc::new(RwLock::new(HashMap::new())),
            protocol_comp: Arc::new(RwLock::new(HashMap::new())),
            graph: Arc::new(RwLock::new(TradingGraph::new())),
            block_number: Arc::new(RwLock::new(0u64)),
        }
    }

    pub async fn update_block_number(&self, block_number: u64) {
        let mut guard = self.block_number.write().await;
        *guard = block_number;
    }

    pub async fn get_block_number(&self) -> u64 {
        *self.block_number.read().await
    }
}

impl Default for MarketDataManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Manages path finding and optimization for arbitrage opportunities.
#[derive(Debug)]
pub struct PathFinder {
    pub paths: Arc<RwLock<PathRepository>>,
    pub source_balances: Arc<RwLock<HashMap<Bytes, BigUint>>>,
    pub optimization_tolerances: HashMap<Bytes, f64>,
    pub source_tokens: Vec<Bytes>,
}

impl PathFinder {
    pub fn new(source_tokens: Vec<Bytes>, optimization_tolerances: HashMap<Bytes, f64>) -> Self {
        Self {
            paths: Arc::new(RwLock::new(PathRepository::new(source_tokens.clone(), 3))),
            source_balances: Arc::new(RwLock::new(HashMap::new())),
            optimization_tolerances,
            source_tokens,
        }
    }
}

/// Manages trade execution including simulation and bundle submission.
pub struct TradeExecutor {
    pub simulator: Arc<Simulator>,
    pub executor: Arc<TxExecutor>,
    pub provider: Arc<RootProvider<Ethereum>>,
    pub signer: PrivateKeySigner,
}

impl TradeExecutor {
    pub fn new(
        simulator: Simulator,
        executor: TxExecutor,
        provider: Arc<RootProvider<Ethereum>>,
        signer: PrivateKeySigner,
    ) -> Self {
        Self {
            simulator: Arc::new(simulator),
            executor: Arc::new(executor),
            provider,
            signer,
        }
    }
}

/// Configuration parameters for arbitrage operations.
#[derive(Debug, Clone)]
pub struct ArbitrageParams {
    pub native_token: Bytes,
    pub min_profit_bps: u64,
}

impl ArbitrageParams {
    pub fn new(native_token: Bytes, min_profit_bps: u64) -> Self {
        Self {
            native_token,
            min_profit_bps,
        }
    }
}

/// Parameters for a single arbitrage search operation.
#[derive(Debug, Clone)]
pub struct SearchParams {
    pub updated_pools: Vec<Bytes>,
    pub block_number: u64,
}

impl SearchParams {
    pub fn new(updated_pools: Vec<Bytes>, block_number: u64) -> Self {
        Self {
            updated_pools,
            block_number,
        }
    }
}

/// Context for market-related operations.
#[derive(Debug)]
pub struct MarketContext<'a> {
    pub market_data: &'a MarketDataManager,
    pub path_finder: &'a PathFinder,
}

impl<'a> MarketContext<'a> {
    pub fn new(market_data: &'a MarketDataManager, path_finder: &'a PathFinder) -> Self {
        Self {
            market_data,
            path_finder,
        }
    }
}

/// Context for execution-related operations.
pub struct ExecutionContext<'a> {
    pub trade_executor: &'a TradeExecutor,
    pub params: &'a ArbitrageParams,
}

impl<'a> ExecutionContext<'a> {
    pub fn new(trade_executor: &'a TradeExecutor, params: &'a ArbitrageParams) -> Self {
        Self {
            trade_executor,
            params,
        }
    }
}
