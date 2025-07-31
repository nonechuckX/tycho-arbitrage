//! Transaction log parsing and event decoding for arbitrage operations.
//!
//! This module provides comprehensive parsing capabilities for blockchain transaction logs,
//! specifically focused on decoding swap events from various decentralized exchange protocols.
//! It supports multiple DEX protocols including Uniswap V2/V3/V4, PancakeSwap V3, Balancer V2, and Curve.
//!
//! # Supported Protocols
//!
//! - **Uniswap V2**: Classic AMM with constant product formula
//! - **Uniswap V3**: Concentrated liquidity with tick-based pricing
//! - **Uniswap V4**: Next-generation AMM with hooks and custom pools
//! - **PancakeSwap V3**: Uniswap V3 fork with protocol fees
//! - **Balancer V2**: Multi-token pools with weighted pricing
//! - **Curve**: Stableswap AMM optimized for low-slippage trades between similar assets
//!
//! # Core Types
//!
//! - **`DecodedSwap`**: Represents a single decoded swap event with amounts and direction
//! - **`DecodedLogs`**: Complete parsing result including all swaps and gas metrics
//! - **`LogParser`**: Main parser that handles protocol detection and event decoding
//!
//! # Event Decoding Process
//!
//! 1. **Protocol Detection**: Identify the DEX protocol from event signatures
//! 2. **Event Parsing**: Decode the raw log data into structured swap information
//! 3. **Direction Analysis**: Determine swap direction (zero_for_one) and amounts
//! 4. **Validation**: Ensure decoded data is consistent and complete
//!
//! # Error Handling
//!
//! The parser is designed to be resilient to unknown protocols and malformed events.
//! It will skip unrecognized events and continue processing, only failing if no
//! valid swap events are found in the expected transaction logs.

use alloy::{
    primitives::U256,
    rpc::types::simulate::SimulatedBlock,
    sol_types::SolEvent,
};
use crate::errors::{SimulationError, Result};
use num_bigint::BigUint;
use tycho_common::Bytes;
use crate::utils::*;

mod uniswap_v2 {
    use alloy::sol;
    sol! {
        #[derive(Debug)]
        event Swap(
            address indexed sender,
            uint amount0In,
            uint amount1In,
            uint amount0Out,
            uint amount1Out,
            address indexed to
        );
    }
}

mod uniswap_v3 {
    use alloy::sol;
    sol! {
        #[derive(Debug)]
        event Swap(
            address indexed sender,
            address indexed recipient,
            int256 amount0,
            int256 amount1,
            uint160 sqrtPriceX96,
            uint128 liquidity, int24 tick
        );
    }
}

mod pancake_v3 {
    use alloy::sol;
    sol! {
        #[derive(Debug)]
        event Swap(
            address indexed sender,
            address indexed recipient,
            int256 amount0,
            int256 amount1,
            uint160 sqrtPriceX96,
            uint128 liquidity,
            int24 tick,
            uint128 protocolFeesToken0,
            uint128 protocolFeesToken1
        );
    }
}

mod balancer_v2 {
    alloy::sol! {
        interface IERC20 {
            function totalSupply() external view returns (uint256);

            function balanceOf(address who) external view returns (uint256);

            function allowance(address owner, address spender) external view returns (uint256);

            function transfer(address to, uint256 value) external returns (bool);

            function approve(address spender, uint256 value) external returns (bool);

            function transferFrom(address from, address to, uint256 value) external returns (bool);

            event Transfer(address indexed from, address indexed to, uint256 value);

            event Approval(address indexed owner, address indexed spender, uint256 value);
        }

        #[derive(Debug)]
        event Swap(
            bytes32 indexed poolId,
            IERC20 indexed tokenIn,
            IERC20 indexed tokenOut,
            uint256 amountIn,
            uint256 amountOut
        );
    }
}

mod uniswap_v4 {
    alloy::sol! {
        type PoolId is bytes32;

        #[derive(Debug)]
        event Swap(
            PoolId indexed id,
            address indexed sender,
            int128 amount0,
            int128 amount1,
            uint160 sqrtPriceX96,
            uint128 liquidity,
            int24 tick,
            uint24 fee
        );
    }
}

mod curve {
    use alloy::sol;
    sol! {
        #[derive(Debug)]
        event TokenExchange(
            address indexed buyer,
            int128 sold_id,
            uint256 tokens_sold,
            int128 bought_id,
            uint256 tokens_bought
        );
    }
}

/// A decoded swap event from a decentralized exchange transaction log.
///
/// This structure represents a single swap operation that was executed on-chain,
/// containing the essential information needed to understand the trade direction,
/// amounts, and pool involved.
#[derive(Debug)]
pub struct DecodedSwap {
    /// The address of the liquidity pool where the swap occurred
    pub pool: Bytes,
    /// Whether this is a zero-for-one swap (token0 -> token1)
    /// 
    /// In most DEX protocols, tokens are ordered by address. A zero_for_one swap
    /// means trading from the lower-addressed token to the higher-addressed token.
    pub zero_for_one: bool,
    /// The amount of input tokens that were swapped
    pub amount_in: BigUint,
    /// The amount of output tokens that were received
    pub amount_out: BigUint,
}

/// Complete parsing result from analyzing simulation transaction logs.
///
/// This structure contains all the decoded swap events from an arbitrage transaction,
/// along with gas usage metrics for both the approval and swap transactions.
#[derive(Debug)]
pub struct DecodedLogs {
    /// Sequence of decoded swaps representing the arbitrage path
    pub path: Vec<DecodedSwap>,
    /// Gas used by the token approval transaction
    pub approval_gas: u64,
    /// Gas used by the swap execution transaction
    pub swap_gas: u64,
}

impl DecodedLogs {
    /// Calculate the profit from the arbitrage path.
    ///
    /// Computes the difference between the final output amount and the initial input amount.
    /// A positive result indicates profit, while a negative result indicates a loss.
    ///
    /// # Returns
    ///
    /// The profit as a signed BigInt, where positive values represent profit
    /// and negative values represent losses.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The path is empty (no swaps were decoded)
    /// - The path contains no valid first or last swap
    pub fn profit(&self) -> Result<num_bigint::BigInt> {
        let last_swap = self.path.last()
            .ok_or_else(|| SimulationError::LogParsingFailed { 
                reason: "Empty path: no swaps available".to_string() 
            })?;
        let first_swap = self.path.first()
            .ok_or_else(|| SimulationError::LogParsingFailed { 
                reason: "Empty path: no swaps available".to_string() 
            })?;
        
        Ok(num_bigint::BigInt::from(last_swap.amount_out.clone())
            - num_bigint::BigInt::from(first_swap.amount_in.clone()))
    }

    /// Calculate the total gas cost for the arbitrage transaction.
    ///
    /// Computes the total gas cost by multiplying the combined gas usage
    /// (approval + swap) by the provided base fee.
    ///
    /// # Arguments
    ///
    /// * `base_fee` - The base fee per gas unit for the transaction
    ///
    /// # Returns
    ///
    /// The total gas cost as a BigUint
    pub fn gas_cost(&self, base_fee: BigUint) -> BigUint {
        BigUint::from(self.approval_gas + self.swap_gas) * base_fee
    }
}

/// Main parser for decoding transaction logs from arbitrage simulations.
///
/// The LogParser provides static methods for parsing simulation results and extracting
/// swap events from various DEX protocols. It handles protocol detection, event decoding,
/// and validation of the resulting arbitrage path.
pub struct LogParser;

impl LogParser {
    /// Parse simulation results to extract decoded swap events and gas metrics.
    ///
    /// This is the main entry point for log parsing. It processes the simulation results,
    /// validates that the simulation was successful, extracts gas usage metrics, and
    /// decodes all swap events from the transaction logs.
    ///
    /// # Arguments
    ///
    /// * `simulated_blocks` - The simulation results from the RPC provider
    ///
    /// # Returns
    ///
    /// A `DecodedLogs` structure containing the parsed swap path and gas metrics.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The simulation failed (transaction reverted)
    /// - No valid swap events could be decoded from the logs
    /// - The decoded path contains fewer than 2 swaps (invalid arbitrage)
    pub fn parse_simulation_results(simulated_blocks: Vec<SimulatedBlock>) -> Result<DecodedLogs> {
        Self::validate_simulation_success(&simulated_blocks)?;
        
        let (approval_gas, swap_gas) = Self::extract_gas_metrics(&simulated_blocks);
        let decoded_path = Self::decode_swap_events(&simulated_blocks)?;
        
        Self::validate_decoded_path(&decoded_path)?;

        Ok(DecodedLogs {
            path: decoded_path,
            approval_gas,
            swap_gas,
        })
    }

    fn validate_simulation_success(simulated_blocks: &[SimulatedBlock]) -> Result<()> {
        let sim_result = &simulated_blocks[0].calls[1];
        if !sim_result.status {
            return Err(SimulationError::SimulationFailed { 
                reason: "Simulation failed".to_string() 
            }.into());
        }
        Ok(())
    }

    fn extract_gas_metrics(simulated_blocks: &[SimulatedBlock]) -> (u64, u64) {
        let approval_gas = simulated_blocks[0].calls[0].gas_used;
        let swap_gas = simulated_blocks[0].calls[1].gas_used;
        (approval_gas, swap_gas)
    }

    fn decode_swap_events(simulated_blocks: &[SimulatedBlock]) -> Result<Vec<DecodedSwap>> {
        let sim_result = &simulated_blocks[0].calls[1];
        let mut decoded_path = Vec::new();

        for log in sim_result.logs.iter() {
            if let Some(decoded_swap) = Self::decode_single_log(log) {
                decoded_path.push(decoded_swap);
            }
        }

        Ok(decoded_path)
    }

    fn decode_single_log(log: &alloy::rpc::types::Log) -> Option<DecodedSwap> {
        // Try each protocol decoder in sequence
        if let Some(swap) = Self::decode_uniswap_v2_swap(log) {
            return Some(swap);
        }
        if let Some(swap) = Self::decode_uniswap_v3_swap(log) {
            return Some(swap);
        }
        if let Some(swap) = Self::decode_uniswap_v4_swap(log) {
            return Some(swap);
        }
        if let Some(swap) = Self::decode_pancake_v3_swap(log) {
            return Some(swap);
        }
        if let Some(swap) = Self::decode_balancer_v2_swap(log) {
            return Some(swap);
        }
        if let Some(swap) = Self::decode_curve_swap(log) {
            return Some(swap);
        }
        None
    }

    fn decode_uniswap_v2_swap(log: &alloy::rpc::types::Log) -> Option<DecodedSwap> {
        if let Ok(swap) = uniswap_v2::Swap::decode_log(&log.inner) {
            let (zero_for_one, amount_in, amount_out) = if swap.amount1Out > U256::ZERO {
                (
                    true,
                    u256_to_biguint(swap.amount0In),
                    u256_to_biguint(swap.amount1Out),
                )
            } else {
                (
                    false,
                    u256_to_biguint(swap.amount1In),
                    u256_to_biguint(swap.amount0Out),
                )
            };
            
            return Some(DecodedSwap {
                pool: Bytes::from(log.inner.address.as_slice()),
                zero_for_one,
                amount_in,
                amount_out,
            });
        }
        None
    }

    fn decode_uniswap_v3_swap(log: &alloy::rpc::types::Log) -> Option<DecodedSwap> {
        if let Ok(swap) = uniswap_v3::Swap::decode_log(&log.inner) {
            let (zero_for_one, amount_in, amount_out) = if swap.amount1.is_positive() {
                (
                    false,
                    i256_to_biguint(swap.amount1),
                    i256_to_biguint(swap.amount0),
                )
            } else {
                (
                    true,
                    i256_to_biguint(swap.amount0),
                    i256_to_biguint(swap.amount1),
                )
            };
            
            return Some(DecodedSwap {
                pool: Bytes::from(log.inner.address.as_slice()),
                zero_for_one,
                amount_in,
                amount_out,
            });
        }
        None
    }

    fn decode_uniswap_v4_swap(log: &alloy::rpc::types::Log) -> Option<DecodedSwap> {
        if let Ok(swap) = uniswap_v4::Swap::decode_log(&log.inner) {
            let (zero_for_one, amount_in, amount_out) = if swap.amount1.is_positive() {
                (
                    false,
                    i128_to_biguint(swap.amount1),
                    i128_to_biguint(swap.amount0),
                )
            } else {
                (
                    true,
                    i128_to_biguint(swap.amount0),
                    i128_to_biguint(swap.amount1),
                )
            };
            
            return Some(DecodedSwap {
                pool: Bytes::from(log.inner.address.as_slice()),
                zero_for_one,
                amount_in,
                amount_out,
            });
        }
        None
    }

    fn decode_pancake_v3_swap(log: &alloy::rpc::types::Log) -> Option<DecodedSwap> {
        if let Ok(swap) = pancake_v3::Swap::decode_log(&log.inner) {
            let (zero_for_one, amount_in, amount_out) = if swap.amount1.is_positive() {
                (
                    false,
                    i256_to_biguint(swap.amount1),
                    i256_to_biguint(swap.amount0),
                )
            } else {
                (
                    true,
                    i256_to_biguint(swap.amount0),
                    i256_to_biguint(swap.amount1),
                )
            };
            
            return Some(DecodedSwap {
                pool: Bytes::from(log.inner.address.as_slice()),
                zero_for_one,
                amount_in,
                amount_out,
            });
        }
        None
    }

    fn decode_balancer_v2_swap(log: &alloy::rpc::types::Log) -> Option<DecodedSwap> {
        if let Ok(swap) = balancer_v2::Swap::decode_log(&log.inner) {
            let zero_for_one = swap.tokenIn < swap.tokenOut;
            return Some(DecodedSwap {
                pool: Bytes::from(swap.poolId.as_slice()),
                zero_for_one,
                amount_in: u256_to_biguint(swap.amountIn),
                amount_out: u256_to_biguint(swap.amountOut),
            });
        }
        None
    }

    fn decode_curve_swap(log: &alloy::rpc::types::Log) -> Option<DecodedSwap> {
        if let Ok(swap) = curve::TokenExchange::decode_log(&log.inner) {
            // In Curve pools, tokens are indexed by integer IDs (0, 1, 2, etc.)
            // zero_for_one is true when swapping from a lower index to a higher index
            let zero_for_one = swap.sold_id < swap.bought_id;
            
            return Some(DecodedSwap {
                pool: Bytes::from(log.inner.address.as_slice()),
                zero_for_one,
                amount_in: u256_to_biguint(swap.tokens_sold),
                amount_out: u256_to_biguint(swap.tokens_bought),
            });
        }
        None
    }

    fn validate_decoded_path(decoded_path: &[DecodedSwap]) -> Result<()> {
        if decoded_path.len() < 2 {
            return Err(SimulationError::InsufficientDecodedLogs { 
                expected: 2, 
                actual: decoded_path.len() 
            }.into());
        }
        Ok(())
    }
}
