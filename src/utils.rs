//! Utility functions and type conversions for blockchain operations.
//!
//! This module provides essential utility functions for working with blockchain data types,
//! chain configurations, and numerical conversions. It serves as a bridge between different
//! type systems used throughout the library, particularly for converting between Alloy types,
//! BigUint, and other numerical representations.
//!
//! # Core Functionality
//!
//! - **Type Conversions**: Safe conversions between U256, I256, BigUint, and primitive types
//! - **Address Handling**: Parsing and validation of Ethereum addresses
//! - **Chain Configuration**: Chain ID mapping and default service URLs
//! - **Fee Calculations**: Base fee calculations for EIP-1559 transactions
//! - **Builder Parameters**: MEV builder configuration for different relayers
//!
//! # Type Safety
//!
//! All conversion functions are designed to handle edge cases and provide clear error
//! messages when conversions fail. The module prioritizes safety over performance,
//! ensuring that invalid data is caught early rather than causing runtime panics.

use alloy::primitives::{Address, U256, I256};
use num_bigint::BigUint;
use std::str::FromStr;
use crate::errors::{Result, UtilityError};
use tycho_common::models::Chain;

/// Convert a signed 256-bit integer to an unsigned BigUint.
///
/// Takes the absolute value of the I256 and converts it to a BigUint,
/// discarding the sign information. This is commonly used when processing
/// DEX swap amounts where the sign indicates direction but we need the magnitude.
///
/// # Arguments
///
/// * `i` - The signed 256-bit integer to convert
///
/// # Returns
///
/// The absolute value as a BigUint
pub fn i256_to_biguint(i: I256) -> BigUint {
    let (_, uint) = i.into_sign_and_abs();
    let bytes = uint.to_be_bytes::<32>();
    
    BigUint::from_bytes_be(&bytes)
}

/// Convert a signed 128-bit integer to an unsigned BigUint.
///
/// Takes the absolute value of the i128 and converts it to a BigUint,
/// discarding the sign information. Used for processing smaller integer
/// values from smart contract events.
///
/// # Arguments
///
/// * `i` - The signed 128-bit integer to convert
///
/// # Returns
///
/// The absolute value as a BigUint
pub fn i128_to_biguint(i: i128) -> BigUint {
    let bytes = i.abs().to_be_bytes();
    
    BigUint::from_bytes_be(&bytes)
}

/// Parse a string representation of an Ethereum address.
///
/// Accepts addresses with or without the "0x" prefix and validates
/// the hex format. The address must be exactly 20 bytes (40 hex characters).
///
/// # Arguments
///
/// * `s` - The string representation of the address
///
/// # Returns
///
/// A parsed Address if the string is valid
///
/// # Errors
///
/// This function will return an error if:
/// - The string contains invalid hex characters
/// - The string is not exactly 40 hex characters (after removing 0x prefix)
/// - The address format is otherwise malformed
pub fn string_to_h160(s: &str) -> Result<Address> { 
    Address::from_str(s.trim_start_matches("0x"))
        .map_err(|source| UtilityError::AddressParsingFailed {
            input: s.to_string(),
            source: alloy::primitives::AddressError::Hex(source),
        }.into())
}

/// Convert a byte slice to an Ethereum address.
///
/// Validates that the byte slice is exactly 20 bytes long and creates
/// an Address from the raw bytes.
///
/// # Arguments
///
/// * `bytes_slice` - The byte slice containing the address data
///
/// # Returns
///
/// A parsed Address if the byte slice is valid
///
/// # Errors
///
/// This function will return an error if:
/// - The byte slice is not exactly 20 bytes long
pub fn bytes_slice_to_h160(bytes_slice: &[u8]) -> Result<Address> {
    if bytes_slice.len() == Address::len_bytes() { 
        Ok(Address::from_slice(bytes_slice))
    } else {
        Err(UtilityError::InvalidAddressLength {
            expected: Address::len_bytes(),
            actual: bytes_slice.len(),
        }.into())
    }
}

/// Convert a U256 value to a BigUint.
///
/// Performs a lossless conversion from Alloy's U256 type to num-bigint's BigUint.
/// This is commonly used when interfacing between different numerical libraries.
///
/// # Arguments
///
/// * `val` - The U256 value to convert
///
/// # Returns
///
/// The equivalent BigUint value
pub fn u256_to_biguint(val: U256) -> BigUint {
    BigUint::from_bytes_be(&val.to_be_bytes::<32>())
}

/// Convert a BigUint to a U256 value.
///
/// Attempts to convert a BigUint to Alloy's U256 type. The conversion will fail
/// if the BigUint value is too large to fit in a 256-bit unsigned integer.
///
/// # Arguments
///
/// * `val` - The BigUint value to convert
///
/// # Returns
///
/// The equivalent U256 value if the conversion is successful
///
/// # Errors
///
/// This function will return an error if:
/// - The BigUint value is larger than 2^256 - 1 (maximum U256 value)
pub fn biguint_to_u256(val: &BigUint) -> Result<U256> {
    let bytes = val.to_bytes_be();
    if bytes.len() > 32 {
        return Err(UtilityError::ValueTooLarge.into());
    }
    let mut u256_bytes = [0u8; 32];
    u256_bytes[32 - bytes.len()..].copy_from_slice(&bytes);
    Ok(U256::from_be_bytes(u256_bytes))
}

/// Get the default Tycho service URL for a given blockchain.
///
/// Returns the default Tycho API endpoint URL for supported chains.
/// These URLs are used for accessing liquidity pool data and protocol information.
///
/// # Arguments
///
/// * `chain` - The blockchain to get the URL for
///
/// # Returns
///
/// The default Tycho URL if the chain is supported, None otherwise
pub fn get_default_tycho_url(chain: &Chain) -> Option<String> {
    match chain {
        Chain::Ethereum => Some("tycho-beta.propellerheads.xyz".to_string()),
        Chain::Base => Some("tycho-base-beta.propellerheads.xyz".to_string()),
        Chain::Unichain => Some("tycho-unichain-beta.propellerheads.xyz".to_string()),
        _ => None, 
    }
}

/// Get the chain ID for a given blockchain name.
///
/// Maps human-readable chain names to their corresponding numeric chain IDs
/// as defined in EIP-155. These IDs are used in transaction signing and
/// network identification.
///
/// # Arguments
///
/// * `chain` - The name of the blockchain (e.g., "ethereum", "base")
///
/// # Returns
///
/// The numeric chain ID if the chain is supported
///
/// # Errors
///
/// This function will return an error if:
/// - The chain name is not recognized or supported
pub fn chain_id(chain: &str) -> Result<u64> {
    match chain {
        "ethereum" => Ok(1),
        "base" => Ok(8453),
        "unichain" => Ok(130),
        _ => Err(UtilityError::UnsupportedChain {
            chain: chain.to_string(),
        }.into()),
    }
}

/// Get the chain name for a given chain ID.
///
/// Maps numeric chain IDs back to their corresponding human-readable names.
/// This is the reverse operation of `chain_id()`.
///
/// # Arguments
///
/// * `chain_id` - The numeric chain ID (e.g., 1, 8453, 130)
///
/// # Returns
///
/// The chain name if the chain ID is supported
///
/// # Errors
///
/// This function will return an error if:
/// - The chain ID is not recognized or supported
pub fn chain_name(chain_id: u64) -> Result<&'static str> {
    match chain_id {
        1 => Ok("ethereum"),
        8453 => Ok("base"),
        130 => Ok("unichain"),
        _ => Err(UtilityError::UnsupportedChain {
            chain: chain_id.to_string(),
        }.into()),
    }
}

/// Get the Permit2 contract address for a given blockchain name.
///
/// Maps human-readable chain names to their corresponding Permit2 contract addresses.
/// Permit2 uses CREATE2 deployment with a specific salt, resulting in the same address
/// across all EVM-compatible chains. However, this function allows for chain-specific
/// overrides if needed in the future.
///
/// # Arguments
///
/// * `chain` - The name of the blockchain (e.g., "ethereum", "base")
///
/// # Returns
///
/// The Permit2 contract address if the chain is supported
///
/// # Errors
///
/// This function will return an error if:
/// - The chain name is not recognized or supported
/// - The address parsing fails (should not happen with hardcoded addresses)
pub fn permit2_address(chain: &str) -> Result<Address> {
    let address_str = match chain {
        "ethereum" => "0x000000000022D473030F116dDEE9F6B43aC78BA3",
        "base" => "0x000000000022D473030F116dDEE9F6B43aC78BA3",
        "unichain" => "0x000000000022D473030F116dDEE9F6B43aC78BA3",
        _ => return Err(UtilityError::UnsupportedChain {
            chain: chain.to_string(),
        }.into()),
    };
    
    Address::from_str(address_str).map_err(|source| {
        UtilityError::AddressParsingFailed {
            input: address_str.to_string(),
            source: alloy::primitives::AddressError::Hex(source),
        }.into()
    })
}

/// Get the list of MEV builder names for a specific relayer.
///
/// Returns the list of block builders that are known to work with the specified
/// relayer endpoint. This information is used for bundle submission targeting
/// specific builders.
///
/// # Arguments
///
/// * `relayer` - The relayer URL to get builder parameters for
///
/// # Returns
///
/// A vector of builder names if the relayer is recognized, None otherwise
pub fn builder_params(relayer: &str) -> Option<Vec<String>> {
    match relayer {
        "https://relay.flashbots.net" => Some(vec![
            "builder0x69".to_string(),
            "rsync".to_string(),
            "fib1.io".to_string(),
            "EigenPhi".to_string(),
            "boba-builder".to_string(),
            "Gambit Labs".to_string(),
            "payload".to_string(),
            "Loki".to_string(),
            "BuildAI".to_string(),
            "JetBuilder".to_string(),
            "tbuilder".to_string(),
            "penguinbuild".to_string(),
            "bobthebuilder".to_string(),
            "BTCS".to_string(),
            "bloXroute".to_string(),
            "Blockbeelder".to_string(),
            "Quasar".to_string(),
            "Eureka".to_string(),
        ]),
        _ => None
    }
}

/// Calculate the next block's base fee using EIP-1559 formula.
///
/// Implements the EIP-1559 base fee adjustment mechanism, which increases
/// the base fee when blocks are above the gas target and decreases it when
/// blocks are below the target. The adjustment is capped at 12.5% per block.
///
/// # Arguments
///
/// * `current_base_fee` - The current block's base fee in wei
/// * `gas_used` - The amount of gas used in the current block
/// * `gas_limit` - The gas limit of the current block
///
/// # Returns
///
/// The calculated base fee for the next block as a U256
///
/// # Formula
///
/// - If gas_used == gas_target: base fee remains unchanged
/// - If gas_used > gas_target: base fee increases by up to 12.5%
/// - If gas_used < gas_target: base fee decreases by up to 12.5%
///
/// Where gas_target = gas_limit / 2
pub fn calculate_next_base_fee(
    current_base_fee: u128,
    gas_used: u128,
    gas_limit: u128,
) -> U256 {
    let gas_target = gas_limit / 2;

    if gas_used == gas_target {
        U256::from(current_base_fee)
    } else if gas_used > gas_target {
        let gas_used_delta = gas_used - gas_target;
        let base_fee_per_gas_delta = current_base_fee * gas_used_delta / gas_target / 8;
        U256::from(current_base_fee + base_fee_per_gas_delta)
    } else {
        let gas_used_delta = gas_target - gas_used;
        let base_fee_per_gas_delta = current_base_fee * gas_used_delta / gas_target / 8;
        U256::from(current_base_fee - base_fee_per_gas_delta)
    }
}
