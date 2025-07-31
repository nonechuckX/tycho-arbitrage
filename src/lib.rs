//! Tycho Atomic Arbitrage Library
//!
//! A high-performance library for executing atomic arbitrage transactions on Ethereum
//! and compatible blockchains. This library provides a complete toolkit for discovering,
//! simulating, and executing profitable arbitrage opportunities across decentralized
//! exchanges and liquidity pools.
//!
//! # Architecture Overview
//!
//! The library is organized into several key modules:
//!
//! - **`graph`**: Token trading graph for modeling liquidity networks
//! - **`path`**: Trading path discovery and optimization algorithms
//! - **`simulation`**: Transaction simulation and validation engine
//! - **`bundle`**: Bundle creation and submission to block builders
//! - **`config`**: Secure configuration management and validation
//! - **`builders`**: Builder patterns for complex object construction
//! - **`errors`**: Comprehensive error handling and reporting
//! - **`utils`**: Utility functions for type conversions and chain operations
//!
//! # Core Concepts
//!
//! - **Trading Graph**: A specialized graph structure where nodes represent tokens
//!   and edges represent liquidity pools or trading pairs
//! - **Arbitrage Path**: A sequence of swaps that starts and ends with the same token,
//!   potentially generating profit from price differences
//! - **Bundle Submission**: Atomic execution of multiple transactions through
//!   block builders like Flashbots
//! - **Simulation**: Pre-execution validation of arbitrage strategies to ensure
//!   profitability and successful execution
//!
//! # Security Considerations
//!
//! This library handles sensitive operations including private key management
//! and transaction signing. Always follow security best practices:
//!
//! - Store private keys securely using environment variables
//! - Use HTTPS-only endpoints for all network communications
//! - Validate all configuration before use
//! - Monitor for failed transactions and handle errors appropriately
//!
//! # Thread Safety
//!
//! Most types in this library are not thread-safe by default. Use appropriate
//! synchronization primitives when sharing instances across threads.

pub mod builders;
pub mod bundle;
pub mod config;
pub mod errors;
pub mod graph;
pub mod path;
pub mod simulation;
pub mod utils;

// Re-export the main Result type and error enum for convenience
pub use errors::{ArbitrageError, Result};

// Re-export builder patterns for convenience
pub use builders::{TradingGraphBuilder, SimulatorBuilder, TxExecutorBuilder};

// Type aliases for commonly used complex types
pub type ProtocolSimulationMap = std::collections::HashMap<tycho_common::Bytes, Box<dyn tycho_simulation::protocol::state::ProtocolSim>>;
pub type ProtocolComponentMap = std::collections::HashMap<tycho_common::Bytes, tycho_simulation::protocol::models::ProtocolComponent>;
pub type NodeIndexMap = std::collections::HashMap<tycho_common::Bytes, usize>;
pub type EdgeIndexMap = std::collections::HashMap<[usize; 2], Vec<usize>>;

// Module-specific result types for better ergonomics
pub type GraphResult<T> = std::result::Result<T, errors::GraphError>;
pub type PathResult<T> = std::result::Result<T, errors::PathError>;
pub type BundleResult<T> = std::result::Result<T, errors::BundleError>;
pub type SimulationResult<T> = std::result::Result<T, errors::SimulationError>;
pub type UtilityResult<T> = std::result::Result<T, errors::UtilityError>;
