//! Simulation and transaction execution errors.

use alloy::primitives::Address;

/// Errors that can occur during simulation operations
#[derive(Debug, thiserror::Error)]
pub enum SimulationError {
    #[error("Simulation failed: {reason}")]
    SimulationFailed { reason: String },

    #[error("Transaction simulation failed with status: {status}")]
    TransactionFailed { status: bool },

    #[error("Failed to build transaction request: {reason}")]
    TransactionBuildFailed { reason: String },

    #[error("Failed to encode solution: {reason}")]
    SolutionEncodingFailed { reason: String },

    #[error("Failed to sign permit: {reason}")]
    PermitSigningFailed { reason: String },

    #[error("Invalid chain configuration: {chain}")]
    InvalidChain { chain: String },

    #[error("Provider error: {message}")]
    ProviderError { message: String },

    #[error("Insufficient gas: required {required}, available {available}")]
    InsufficientGas { required: u64, available: u64 },

    #[error("Invalid nonce: {nonce}")]
    InvalidNonce { nonce: u64 },

    #[error("Base fee calculation failed: {reason}")]
    BaseFeeCalculationFailed { reason: String },

    #[error("Router address not found")]
    RouterAddressNotFound,

    #[error("Invalid router calldata")]
    InvalidRouterCalldata,

    #[error("Permit2 address invalid: {address}")]
    InvalidPermit2Address { address: String },

    #[error("Token approval failed for token {token:?}")]
    TokenApprovalFailed { token: Address },

    #[error("Swap execution failed: {reason}")]
    SwapExecutionFailed { reason: String },

    #[error("Log parsing failed: {reason}")]
    LogParsingFailed { reason: String },

    #[error("Insufficient decoded logs: expected at least {expected}, got {actual}")]
    InsufficientDecodedLogs { expected: usize, actual: usize },

    #[error("Protocol not supported: {protocol}")]
    UnsupportedProtocol { protocol: String },

    #[error("Invalid swap event data")]
    InvalidSwapEventData,

    #[error("Gas estimation failed: {reason}")]
    GasEstimationFailed { reason: String },

    #[error("Simulation timeout after {timeout_ms}ms")]
    SimulationTimeout { timeout_ms: u64 },

    #[error("Invalid simulation payload")]
    InvalidSimulationPayload,

    #[error("Simulation result validation failed: {reason}")]
    ValidationFailed { reason: String },
}
