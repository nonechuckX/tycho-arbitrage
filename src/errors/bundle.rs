//! Bundle execution and transaction-related errors.

/// Errors that can occur during bundle operations
#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    #[error("Invalid private key format: {message}")]
    InvalidPrivateKey { message: String },

    #[error("Transaction signing failed: {reason}")]
    TransactionSigningFailed { reason: String },

    #[error("Failed to connect to relayer {url}: {error}")]
    RelayerConnectionFailed { url: String, error: String },

    #[error("Invalid transaction count: expected {expected}, got {actual}")]
    InvalidTransactionCount { expected: usize, actual: usize },

    #[error("Failed to build typed transaction: {reason}")]
    TransactionBuildFailed { reason: String },

    #[error("Failed to encode transaction: {reason}")]
    TransactionEncodingFailed { reason: String },

    #[error("Bundle submission failed for all relayers")]
    AllRelayersFailed,

    #[error("Invalid bundle configuration: {message}")]
    InvalidConfiguration { message: String },

    #[error("Request signing failed: {reason}")]
    RequestSigningFailed { reason: String },

    #[error("Invalid response from relayer {url}: {message}")]
    InvalidRelayerResponse { url: String, message: String },

    #[error("Insufficient bribe amount: {amount} is below minimum")]
    InsufficientBribe { amount: String },

    #[error("Target block {block} is in the past")]
    InvalidTargetBlock { block: u64 },
}
