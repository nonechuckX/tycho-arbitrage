//! Comprehensive error handling and reporting for the arbitrage library.
//!
//! This module provides a hierarchical error system with fine-grained error types
//! for each major component of the library. The error system is designed to provide
//! clear, actionable error messages while maintaining type safety and enabling
//! sophisticated error handling patterns.
//!
//! # Error Hierarchy
//!
//! The error system is organized into domain-specific error types:
//!
//! - **`BundleError`**: Errors related to transaction bundle creation and submission
//! - **`GraphError`**: Errors in trading graph operations and validation
//! - **`PathError`**: Errors in arbitrage path discovery and execution
//! - **`SimulationError`**: Errors during transaction simulation and validation
//! - **`UtilityError`**: Errors in utility functions and type conversions
//!
//! # Top-Level Error Type
//!
//! The `ArbitrageError` enum serves as the top-level error type that encompasses
//! all possible errors from the library and its dependencies. It provides automatic
//! conversion from all domain-specific errors and external library errors.
//!
//! # Error Handling Patterns
//!
//! The library uses `Result<T>` return types consistently, with the `Result` type
//! alias defaulting to `ArbitrageError`. This enables:
//!
//! - **Error Propagation**: Using the `?` operator for clean error handling
//! - **Pattern Matching**: Matching on specific error types for targeted handling
//! - **Error Context**: Rich error messages with context about what operation failed
//! - **Error Recovery**: Structured error information for implementing retry logic
//!
//! # External Error Integration
//!
//! The error system integrates with errors from external dependencies including:
//! - Network errors from HTTP requests
//! - Serialization errors from JSON processing
//! - Cryptographic errors from signing operations
//! - RPC errors from blockchain interactions
//! - Encoding errors from transaction construction

pub mod bundle;
pub mod graph;
pub mod path;
pub mod simulation;
pub mod utility;

// Re-export all error types for convenience
pub use bundle::BundleError;
pub use graph::GraphError;
pub use path::PathError;
pub use simulation::SimulationError;
pub use utility::UtilityError;

/// Main result type for the library
pub type Result<T> = std::result::Result<T, ArbitrageError>;

/// Top-level error enum that encompasses all possible errors in the arbitrage library.
///
/// This enum serves as the unified error type for the entire library, providing
/// automatic conversion from all domain-specific errors and external dependencies.
/// It enables comprehensive error handling while maintaining clear error categorization.
///
/// # Error Categories
///
/// - **Domain Errors**: Errors from specific library components (Bundle, Graph, Path, etc.)
/// - **External Errors**: Errors from dependencies (Network, Serialization, Cryptography)
/// - **System Errors**: Low-level errors from the runtime environment
///
/// # Usage Patterns
///
/// This error type supports various error handling patterns:
/// - Direct matching on error variants for specific handling
/// - Generic error propagation using the `?` operator
/// - Error logging and monitoring with structured error information
/// - Error recovery based on error category and context
#[derive(Debug, thiserror::Error)]
pub enum ArbitrageError {
    /// Error in bundle creation, validation, or submission operations.
    ///
    /// This includes errors in transaction signing, bundle formatting,
    /// relayer communication, and bundle validation failures.
    #[error("Bundle operation failed: {0}")]
    Bundle(#[from] BundleError),

    /// Error in trading graph operations or validation.
    ///
    /// This includes errors in graph construction, node/edge operations,
    /// graph traversal, and structural validation failures.
    #[error("Graph operation failed: {0}")]
    Graph(#[from] GraphError),

    /// Error in arbitrage path discovery, validation, or execution.
    ///
    /// This includes errors in path finding algorithms, path validation,
    /// profitability calculations, and path optimization failures.
    #[error("Path operation failed: {0}")]
    Path(#[from] PathError),

    /// Error during transaction simulation or validation.
    ///
    /// This includes errors in simulation setup, execution failures,
    /// result parsing, and validation of simulation outcomes.
    #[error("Simulation error: {0}")]
    Simulation(#[from] SimulationError),

    /// Error in utility functions or type conversions.
    ///
    /// This includes errors in numerical conversions, address parsing,
    /// chain configuration, and other utility operations.
    #[error("Utility error: {0}")]
    Utility(#[from] UtilityError),

    /// Network communication error.
    ///
    /// This includes HTTP request failures, connection timeouts,
    /// DNS resolution failures, and other network-related issues.
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// JSON serialization or deserialization error.
    ///
    /// This includes errors in parsing JSON responses, serializing
    /// request data, and handling malformed JSON structures.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Cryptographic signing error from Alloy.
    ///
    /// This includes errors in transaction signing, key validation,
    /// and other cryptographic operations.
    #[error("Alloy error: {0}")]
    Alloy(#[from] alloy::signers::Error),

    /// Local signer error for private key operations.
    ///
    /// This includes errors in private key parsing, validation,
    /// and local signing operations.
    #[error("Local signer error: {0}")]
    LocalSigner(#[from] alloy::signers::local::LocalSignerError),

    /// Hexadecimal string parsing error.
    ///
    /// This includes errors in parsing hex-encoded data such as
    /// addresses, transaction hashes, and other hex strings.
    #[error("Hex parsing error: {0}")]
    HexParsing(#[from] alloy::hex::FromHexError),

    /// Transaction encoding error from Tycho execution.
    ///
    /// This includes errors in encoding transactions, solutions,
    /// and other execution-related data structures.
    #[error("Encoding error: {0}")]
    Encoding(#[from] tycho_execution::encoding::errors::EncodingError),

    /// RPC communication error with blockchain nodes.
    ///
    /// This includes errors in RPC requests, response parsing,
    /// connection failures, and blockchain interaction issues.
    #[error("RPC error: {0}")]
    Rpc(#[from] alloy::transports::RpcError<alloy::transports::TransportErrorKind>),

    /// Generic error for cases not covered by specific error types.
    ///
    /// This serves as a fallback for unexpected errors and errors
    /// from dependencies that don't have specific handling.
    #[error("Generic error: {0}")]
    Other(#[from] anyhow::Error),
}
