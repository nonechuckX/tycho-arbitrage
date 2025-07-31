//! Swap types and operations for trading paths.
//!
//! This module provides the fundamental swap abstractions used throughout the arbitrage
//! system. It defines different representations of swaps at various stages of the
//! arbitrage process, from initial path discovery to final execution.
//!
//! # Core Types
//!
//! - **`Swap`**: A basic swap operation representing a potential trade between two tokens
//! - **`SwapExt`**: An executed swap with concrete amounts and gas costs calculated
//! - **`SwapForStorage`**: A lightweight, serializable representation for persistence
//!
//! # Swap Direction
//!
//! All swap types use the `zero_for_one` boolean to indicate trading direction:
//! - `true`: Trading from token0 to token1 (tokens ordered by address)
//! - `false`: Trading from token1 to token0
//!
//! This convention is consistent with most DEX protocols and enables efficient
//! path representation and execution.
//!
//! # Protocol Integration
//!
//! The swap types integrate with the Tycho protocol simulation system to:
//! - Calculate spot prices and liquidity limits
//! - Simulate swap outcomes with gas estimation
//! - Validate swap feasibility before execution
//! - Handle protocol-specific swap mechanics
//!
//! # Error Handling
//!
//! Swap operations can fail due to:
//! - Insufficient liquidity in the pool
//! - Invalid token pairs or amounts
//! - Protocol simulation failures
//! - Spot price calculation errors

use crate::errors::{PathError, Result};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use std::fmt;
use tycho_common::Bytes;
use tycho_simulation::{
    models::Token,
    protocol::{
        models::{GetAmountOutResult, ProtocolComponent},
        state::ProtocolSim,
    },
};

/// A swap operation between two tokens through a liquidity pool.
///
/// This represents a potential swap that can be executed as part of an arbitrage path.
/// It contains the protocol component information and simulation state needed to
/// calculate swap outcomes and validate feasibility.
///
/// # Fields
///
/// - `pool_comp`: The protocol component containing pool and token information
/// - `pool_sim`: The protocol simulation state for calculating swap outcomes
/// - `zero_for_one`: The direction of the swap (token0 -> token1 if true)
#[derive(Clone)]
pub struct Swap {
    /// The protocol component containing pool metadata and token information
    pub pool_comp: ProtocolComponent,
    /// The protocol simulation state for this pool
    pub pool_sim: Box<dyn ProtocolSim>,
    /// Whether this swap goes from token0 to token1 (true) or token1 to token0 (false)
    pub zero_for_one: bool,
}

impl Swap {
    /// Get the input token for this swap.
    ///
    /// Returns the token that will be consumed in this swap operation,
    /// determined by the swap direction and token ordering.
    ///
    /// # Returns
    ///
    /// A reference to the input token
    pub fn token_in(&self) -> &Token {
        if self.zero_for_one {
            &self.pool_comp.tokens[0]
        } else {
            &self.pool_comp.tokens[1]
        }
    }

    /// Get the output token for this swap.
    ///
    /// Returns the token that will be received from this swap operation,
    /// determined by the swap direction and token ordering.
    ///
    /// # Returns
    ///
    /// A reference to the output token
    pub fn token_out(&self) -> &Token {
        if self.zero_for_one {
            &self.pool_comp.tokens[1]
        } else {
            &self.pool_comp.tokens[0]
        }
    }

    /// Calculate the current spot price for this swap.
    ///
    /// The spot price represents the instantaneous exchange rate between
    /// the input and output tokens at the current pool state, without
    /// considering slippage from large trades.
    ///
    /// # Returns
    ///
    /// The spot price as a floating-point ratio
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The protocol simulation fails to calculate the spot price
    /// - The pool state is invalid or corrupted
    /// - The token pair is not supported by the protocol
    pub fn spot_price(&self) -> Result<f64> {
        self.pool_sim
            .spot_price(self.token_in(), self.token_out())
            .map_err(|_| PathError::SpotPriceCalculationFailed { 
                pool: self.pool_comp.id.clone() 
            }.into())
    }

    /// Get the liquidity limits for this swap.
    ///
    /// Returns the maximum amounts that can be traded in each direction
    /// based on the current pool liquidity. This is used to validate
    /// that proposed swap amounts are feasible.
    ///
    /// # Returns
    ///
    /// A tuple containing (max_input_amount, max_output_amount)
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The protocol simulation fails to calculate limits
    /// - The pool has insufficient liquidity
    /// - The token addresses are invalid
    pub fn get_limits(&self) -> Result<(BigUint, BigUint)> {
        self.pool_sim
            .get_limits(
                self.token_in().address.clone(),
                self.token_out().address.clone(),
            )
            .map_err(|_| PathError::InsufficientLiquidity { 
                pool: self.pool_comp.id.clone() 
            }.into())
    }

    /// Calculate the output amount for a given input amount.
    ///
    /// Simulates the swap execution to determine how many output tokens
    /// would be received for the specified input amount, including
    /// slippage and fees. Also estimates the gas cost for execution.
    ///
    /// # Arguments
    ///
    /// * `amount_in` - The amount of input tokens to swap
    ///
    /// # Returns
    ///
    /// A `GetAmountOutResult` containing the output amount and gas estimate
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The input amount exceeds available liquidity
    /// - The protocol simulation fails
    /// - The swap would result in zero output (due to fees or slippage)
    pub fn get_amount_out(&self, amount_in: BigUint) -> Result<GetAmountOutResult> {
        self.pool_sim
            .get_amount_out(amount_in, self.token_in(), self.token_out())
            .map_err(|_| PathError::InsufficientLiquidity { 
                pool: self.pool_comp.id.clone() 
            }.into())
    }
}

impl fmt::Debug for Swap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Swap")
            .field("pool", &self.pool_comp.id)
            .field("protocol_system", &self.pool_comp.protocol_system)
            .field("protocol_type", &self.pool_comp.protocol_type_name)
            .field("zero_for_one", &self.zero_for_one)
            .field("token_in", &self.token_in().address)
            .field("token_out", &self.token_out().address)
            .finish()
    }
}

/// An executed swap with specific amounts and gas costs.
///
/// This represents a swap that has been simulated or executed with concrete
/// input/output amounts and gas costs calculated. It extends the basic Swap
/// with execution-specific information needed for transaction construction.
///
/// # Fields
///
/// - `pool_comp`: The protocol component containing pool and token information
/// - `pool_sim`: The protocol simulation state for calculating additional metrics
/// - `zero_for_one`: The direction of the swap
/// - `amount_in`: The actual amount of input tokens consumed
/// - `amount_out`: The actual amount of output tokens received
/// - `gas`: The estimated gas cost for executing this swap
#[derive(Clone)]
pub struct SwapExt {
    /// The protocol component containing pool metadata and token information
    pub pool_comp: ProtocolComponent,
    /// The protocol simulation state for this pool
    pub pool_sim: Box<dyn ProtocolSim>,
    /// Whether this swap goes from token0 to token1 (true) or token1 to token0 (false)
    pub zero_for_one: bool,
    /// The amount of input tokens consumed in this swap
    pub amount_in: BigUint,
    /// The amount of output tokens received from this swap
    pub amount_out: BigUint,
    /// The estimated gas cost for executing this swap
    pub gas: BigUint,
}

impl SwapExt {
    /// Get the input token for this executed swap.
    ///
    /// Returns the token that was consumed in this swap operation,
    /// determined by the swap direction and token ordering.
    ///
    /// # Returns
    ///
    /// A reference to the input token
    pub fn token_in(&self) -> &Token {
        if self.zero_for_one {
            &self.pool_comp.tokens[0]
        } else {
            &self.pool_comp.tokens[1]
        }
    }

    /// Get the output token for this executed swap.
    ///
    /// Returns the token that was received from this swap operation,
    /// determined by the swap direction and token ordering.
    ///
    /// # Returns
    ///
    /// A reference to the output token
    pub fn token_out(&self) -> &Token {
        if self.zero_for_one {
            &self.pool_comp.tokens[1]
        } else {
            &self.pool_comp.tokens[0]
        }
    }
}

impl fmt::Debug for SwapExt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SwapExt")
            .field("pool", &self.pool_comp.id)
            .field("protocol_system", &self.pool_comp.protocol_system)
            .field("protocol_type", &self.pool_comp.protocol_type_name)
            .field("zero_for_one", &self.zero_for_one)
            .field("amount_in", &self.amount_in)
            .field("amount_out", &self.amount_out)
            .field("gas", &self.gas)
            .finish()
    }
}

/// A serializable representation of a swap for storage purposes.
///
/// This lightweight structure contains only the essential information needed
/// to identify a swap operation without the heavy protocol simulation state.
/// It's designed for persistence, caching, and network transmission scenarios
/// where the full Swap or SwapExt types would be impractical.
///
/// # Fields
///
/// - `pool`: The address of the liquidity pool
/// - `token_in`: The address of the input token
/// - `token_out`: The address of the output token
///
/// # Usage
///
/// This type is typically used for:
/// - Storing arbitrage paths in databases
/// - Caching successful path discoveries
/// - Transmitting path information over networks
/// - Logging and analytics purposes
///
/// The stored information can later be used to reconstruct full Swap objects
/// by looking up the protocol components and simulation state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapForStorage {
    /// The address of the liquidity pool where the swap occurs
    pub pool: Bytes,
    /// The address of the input token
    pub token_in: Bytes,
    /// The address of the output token
    pub token_out: Bytes,
}
