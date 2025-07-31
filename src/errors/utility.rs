//! Utility function errors

use thiserror::Error;

/// Errors that can occur in utility functions
#[derive(Debug, Error)]
pub enum UtilityError {
    #[error("Failed to parse address from string '{input}': {source}")]
    AddressParsingFailed {
        input: String,
        #[source]
        source: alloy::primitives::AddressError,
    },

    #[error("Invalid byte length for address: expected {expected}, got {actual}")]
    InvalidAddressLength { expected: usize, actual: usize },

    #[error("BigUint value too large to fit in U256")]
    ValueTooLarge,

    #[error("Unsupported chain: {chain}")]
    UnsupportedChain { chain: String },
}
