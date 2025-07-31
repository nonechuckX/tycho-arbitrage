//! Path finding and optimization errors.

use tycho_common::Bytes;

/// Errors that can occur during path operations
#[derive(Debug, thiserror::Error)]
pub enum PathError {
    #[error("Path optimization failed: {reason}")]
    OptimizationFailed { reason: String },

    #[error("Invalid path: {reason}")]
    InvalidPath { reason: String },

    #[error("Path too short: minimum length is {min_length}, got {actual_length}")]
    PathTooShort { min_length: usize, actual_length: usize },

    #[error("Path too long: maximum length is {max_length}, got {actual_length}")]
    PathTooLong { max_length: usize, actual_length: usize },

    #[error("No profitable paths found")]
    NoProfitablePaths,

    #[error("Amount exceeds pool limits: requested {requested}, max available {max_available}")]
    AmountExceedsLimits { requested: String, max_available: String },

    #[error("Insufficient liquidity in pool {pool:?}")]
    InsufficientLiquidity { pool: Bytes },

    #[error("Token mismatch in path: expected {expected:?}, got {actual:?}")]
    TokenMismatch { expected: Bytes, actual: Bytes },

    #[error("Spot price calculation failed for pool {pool:?}")]
    SpotPriceCalculationFailed { pool: Bytes },

    #[error("Path repository operation failed: {operation}")]
    RepositoryOperationFailed { operation: String },

    #[error("Pool not found in path repository: {pool:?}")]
    PoolNotFoundInRepository { pool: Bytes },

    #[error("Invalid path index: {index}")]
    InvalidPathIndex { index: usize },

    #[error("Path extension failed: {reason}")]
    ExtensionFailed { reason: String },

    #[error("Ternary search failed: {reason}")]
    TernarySearchFailed { reason: String },

    #[error("Empty path: no swaps available")]
    EmptyPath,

    #[error("Cycle detection failed: path does not form a valid cycle")]
    InvalidCycle,

    #[error("Protocol component not found for pool {pool:?}")]
    ProtocolComponentNotFound { pool: Bytes },

    #[error("Protocol simulation not found for pool {pool:?}")]
    ProtocolSimulationNotFound { pool: Bytes },
}
