//! Path execution and profit calculation logic for atomic arbitrage.
//!
//! This module provides functionality for executing trading paths with specific amounts,
//! calculating profits and losses, and managing execution metrics. It separates the
//! concerns of path execution from path creation and optimization.

use crate::errors::{PathError, Result};
use crate::path::{Path, PathExt, SwapExt};
use num_bigint::{BigInt, BigUint};
use num_traits::Zero;
use std::fmt;

/// Executor for trading paths with specific input amounts.
///
/// The `PathExecutor` handles the execution of trading paths, converting
/// a `Path` into a `PathExt` with concrete amounts and gas costs calculated.
pub struct PathExecutor {
    /// Whether to validate limits before execution
    validate_limits: bool,
    /// Whether to collect detailed execution metrics
    collect_metrics: bool,
}

impl PathExecutor {
    /// Create a new path executor with default settings.
    pub fn new() -> Self {
        Self {
            validate_limits: true,
            collect_metrics: false,
        }
    }

    /// Create a path executor that skips limit validation.
    ///
    /// This can be useful for testing or when limits have already been validated.
    pub fn without_limit_validation() -> Self {
        Self {
            validate_limits: false,
            collect_metrics: false,
        }
    }

    /// Enable detailed execution metrics collection.
    pub fn with_metrics(mut self) -> Self {
        self.collect_metrics = true;
        self
    }

    /// Execute a path with a specific input amount.
    ///
    /// This method simulates the execution of each swap in the path sequentially,
    /// calculating the output amounts and gas costs for each step.
    ///
    /// # Arguments
    ///
    /// * `path` - The trading path to execute
    /// * `amount_in` - The initial input amount for the first swap
    ///
    /// # Returns
    ///
    /// A `PathExt` containing the executed swaps with concrete amounts and gas costs
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The path is empty
    /// - Any swap in the path fails to execute
    /// - The input amount exceeds available liquidity (if validation is enabled)
    pub fn execute_with_amount(&self, path: &Path, amount_in: BigUint) -> Result<PathExt> {
        if path.is_empty() {
            return Err(PathError::EmptyPath.into());
        }

        tracing::debug!(
            path_length = path.len(),
            input_amount = %amount_in,
            validate_limits = self.validate_limits,
            "Executing path with specific amount"
        );

        let mut current_amount = amount_in.clone();
        let mut executed_swaps = Vec::with_capacity(path.len());
        let mut total_gas = BigUint::from(0u32);

        for (index, swap) in path.iter().enumerate() {
            let swap_input = current_amount.clone();

            // Validate limits if enabled
            if self.validate_limits {
                self.validate_swap_limits(swap, &swap_input)?;
            }

            // Execute the swap
            let swap_result = swap.get_amount_out(swap_input.clone()).map_err(|_| {
                PathError::ExtensionFailed {
                    reason: format!("Swap {} failed to execute", index),
                }
            })?;

            let executed_swap = SwapExt {
                pool_comp: swap.pool_comp.clone(),
                pool_sim: swap.pool_sim.clone(),
                zero_for_one: swap.zero_for_one,
                amount_in: swap_input,
                amount_out: swap_result.amount.clone(),
                gas: swap_result.gas.clone(),
            };

            current_amount = swap_result.amount;
            total_gas += &swap_result.gas;
            
            tracing::trace!(
                swap_index = index,
                input_amount = %executed_swap.amount_in,
                output_amount = %executed_swap.amount_out,
                gas_cost = %executed_swap.gas,
                "Swap executed successfully"
            );
            
            executed_swaps.push(executed_swap);
        }

        let path_ext = PathExt(executed_swaps);

        if self.collect_metrics {
            self.log_execution_metrics(&path_ext, &amount_in, &total_gas);
        }

        tracing::debug!(
            path_length = path_ext.len(),
            initial_amount = %amount_in,
            final_amount = %current_amount,
            total_gas = %total_gas,
            "Path execution completed successfully"
        );

        Ok(path_ext)
    }

    /// Calculate the profit/loss for a given input amount without full execution.
    ///
    /// This is a more efficient method when you only need the profit calculation
    /// without the full execution details.
    ///
    /// # Arguments
    ///
    /// * `path` - The trading path to analyze
    /// * `amount_in` - The input amount to calculate profit for
    ///
    /// # Returns
    ///
    /// The profit (positive) or loss (negative) as a BigInt
    pub fn calculate_profit_loss(&self, path: &Path, amount_in: BigUint) -> Result<BigInt> {
        path.calculate_profit_loss(amount_in)
    }

    /// Validate that a swap can handle the requested input amount.
    fn validate_swap_limits(&self, swap: &crate::path::Swap, amount_in: &BigUint) -> Result<()> {
        let (max_in, _max_out) = swap.get_limits()?;

        if max_in < *amount_in {
            return Err(PathError::AmountExceedsLimits {
                requested: amount_in.to_string(),
                max_available: max_in.to_string(),
            }.into());
        }

        Ok(())
    }

    /// Log detailed execution metrics.
    fn log_execution_metrics(&self, path_ext: &PathExt, initial_amount: &BigUint, total_gas: &BigUint) {
        if let (Ok(profit), Ok(is_profitable)) = (path_ext.profit(), path_ext.is_profitable()) {
            tracing::info!(
                path_length = path_ext.len(),
                initial_amount = %initial_amount,
                final_amount = %path_ext.last().map(|s| &s.amount_out).unwrap_or(&BigUint::from(0u32)),
                profit = %profit,
                is_profitable = is_profitable,
                total_gas = %total_gas,
                average_gas_per_swap = %if path_ext.len() > 0 { total_gas / path_ext.len() } else { BigUint::from(0u32) },
                "Path execution metrics"
            );
        }
    }
}

impl Default for PathExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculator for profit and profitability metrics.
pub struct ProfitCalculator;

impl ProfitCalculator {
    /// Calculate the absolute profit from an executed path.
    ///
    /// Returns the difference between the final output amount and initial input amount.
    /// Positive values indicate profit, negative values indicate loss.
    pub fn calculate_absolute_profit(path_ext: &PathExt) -> Result<BigInt> {
        path_ext.profit()
    }

    /// Calculate the profit percentage from an executed path.
    ///
    /// Returns the profit as a percentage of the initial investment.
    /// For example, a return of 0.05 means 5% profit.
    pub fn calculate_profit_percentage(path_ext: &PathExt) -> Result<f64> {
        let first_swap = path_ext.first()
            .ok_or_else(|| PathError::EmptyPath)?;
        let last_swap = path_ext.last()
            .ok_or_else(|| PathError::EmptyPath)?;

        if first_swap.amount_in.is_zero() {
            return Ok(0.0);
        }

        let input_f64 = Self::biguint_to_f64(&first_swap.amount_in);
        let output_f64 = Self::biguint_to_f64(&last_swap.amount_out);

        let profit_percentage = (output_f64 - input_f64) / input_f64;
        Ok(profit_percentage)
    }

    /// Calculate the return on investment (ROI) from an executed path.
    ///
    /// ROI is calculated as (Final Value - Initial Value) / Initial Value * 100
    /// Returns the ROI as a percentage.
    pub fn calculate_roi_percentage(path_ext: &PathExt) -> Result<f64> {
        let profit_percentage = Self::calculate_profit_percentage(path_ext)?;
        Ok(profit_percentage * 100.0)
    }

    /// Check if a path execution is profitable after accounting for gas costs.
    ///
    /// # Arguments
    ///
    /// * `path_ext` - The executed path
    /// * `gas_price` - The gas price in wei per gas unit
    /// * `token_price_in_eth` - The price of the traded token in ETH
    ///
    /// # Returns
    ///
    /// True if the profit exceeds the gas costs, false otherwise
    pub fn is_profitable_after_gas(
        path_ext: &PathExt,
        gas_price: &BigUint,
        token_price_in_eth: f64,
    ) -> Result<bool> {
        let profit = Self::calculate_absolute_profit(path_ext)?;
        
        // Only consider positive profits
        if profit <= BigInt::from(0) {
            return Ok(false);
        }

        let total_gas: BigUint = path_ext.iter().map(|s| &s.gas).sum();
        let gas_cost_wei = total_gas * gas_price;
        let gas_cost_eth = Self::biguint_to_f64(&gas_cost_wei) / 1e18; // Convert wei to ETH
        
        let profit_f64 = Self::bigint_to_f64(&profit);
        let profit_in_eth = profit_f64 * token_price_in_eth / 1e18; // Assuming token has 18 decimals

        Ok(profit_in_eth > gas_cost_eth)
    }

    /// Convert BigUint to f64 for calculations.
    fn biguint_to_f64(value: &BigUint) -> f64 {
        // This is a simplified conversion that may lose precision for very large numbers
        // In production, you might want to use a more sophisticated conversion
        value.to_string().parse().unwrap_or(0.0)
    }

    /// Convert BigInt to f64 for calculations.
    fn bigint_to_f64(value: &BigInt) -> f64 {
        // This is a simplified conversion that may lose precision for very large numbers
        value.to_string().parse().unwrap_or(0.0)
    }
}

/// Execution metrics for performance tracking.
#[derive(Debug, Clone)]
pub struct ExecutionMetrics {
    /// Total gas cost for the entire path
    pub total_gas: BigUint,
    /// Average gas cost per swap
    pub average_gas_per_swap: BigUint,
    /// Number of swaps executed
    pub swap_count: usize,
    /// Initial input amount
    pub initial_amount: BigUint,
    /// Final output amount
    pub final_amount: BigUint,
    /// Calculated profit/loss
    pub profit: BigInt,
    /// Whether the execution was profitable
    pub is_profitable: bool,
}

impl ExecutionMetrics {
    /// Create execution metrics from a completed path execution.
    pub fn from_path_ext(path_ext: &PathExt) -> Result<Self> {
        let total_gas: BigUint = path_ext.iter().map(|s| &s.gas).sum();
        let average_gas_per_swap = if path_ext.len() > 0 {
            &total_gas / path_ext.len()
        } else {
            BigUint::from(0u32)
        };

        let initial_amount = path_ext.first()
            .map(|s| s.amount_in.clone())
            .unwrap_or_else(|| BigUint::from(0u32));

        let final_amount = path_ext.last()
            .map(|s| s.amount_out.clone())
            .unwrap_or_else(|| BigUint::from(0u32));

        let profit = path_ext.profit()?;
        let is_profitable = path_ext.is_profitable()?;

        Ok(Self {
            total_gas,
            average_gas_per_swap,
            swap_count: path_ext.len(),
            initial_amount,
            final_amount,
            profit,
            is_profitable,
        })
    }

    /// Get the profit percentage.
    pub fn profit_percentage(&self) -> f64 {
        if self.initial_amount.is_zero() {
            return 0.0;
        }

        let initial_f64 = ProfitCalculator::biguint_to_f64(&self.initial_amount);
        let final_f64 = ProfitCalculator::biguint_to_f64(&self.final_amount);

        (final_f64 - initial_f64) / initial_f64
    }
}

impl fmt::Display for ExecutionMetrics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ExecutionMetrics {{ swaps: {}, profit: {}, profitable: {}, total_gas: {}, profit_pct: {:.2}% }}",
            self.swap_count,
            self.profit,
            self.is_profitable,
            self.total_gas,
            self.profit_percentage() * 100.0
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::{Path, Swap};
    use std::collections::HashMap;
    use tycho_common::Bytes;
    use tycho_simulation::protocol::models::ProtocolComponent;
    use tycho_simulation::protocol::state::ProtocolSim;
    use std::str::FromStr;

    // Mock ProtocolSim for testing
    #[derive(Debug, Clone)]
    struct MockProtocolSim {
        multiplier: f64,
    }

    impl MockProtocolSim {
        fn new(multiplier: f64) -> Self {
            Self { multiplier }
        }
    }

    impl ProtocolSim for MockProtocolSim {
        fn clone_box(&self) -> Box<dyn ProtocolSim> {
            Box::new(self.clone())
        }

        fn fee(&self) -> f64 {
            0.003
        }

        fn spot_price(
            &self,
            _token_in: &tycho_simulation::models::Token,
            _token_out: &tycho_simulation::models::Token,
        ) -> std::result::Result<f64, tycho_simulation::protocol::errors::SimulationError> {
            Ok(self.multiplier)
        }

        fn get_amount_out(
            &self,
            amount_in: BigUint,
            _token_in: &tycho_simulation::models::Token,
            _token_out: &tycho_simulation::models::Token,
        ) -> std::result::Result<tycho_simulation::protocol::models::GetAmountOutResult, tycho_simulation::protocol::errors::SimulationError> {
            let amount_out = if self.multiplier >= 1.0 {
                &amount_in * BigUint::from((self.multiplier * 1000.0) as u32) / BigUint::from(1000u32)
            } else {
                &amount_in * BigUint::from((self.multiplier * 1000.0) as u32) / BigUint::from(1000u32)
            };

            Ok(tycho_simulation::protocol::models::GetAmountOutResult {
                amount: amount_out,
                gas: BigUint::from(21000u32),
                new_state: Box::new(self.clone()),
            })
        }

        fn get_limits(
            &self,
            _token_in: Bytes,
            _token_out: Bytes,
        ) -> std::result::Result<(BigUint, BigUint), tycho_simulation::protocol::errors::SimulationError> {
            Ok((BigUint::from(1000000u32), BigUint::from(1000000u32)))
        }

        fn delta_transition(
            &mut self,
            _delta: tycho_common::dto::ProtocolStateDelta,
            _tokens: &std::collections::HashMap<Bytes, tycho_simulation::models::Token>,
            _balances: &tycho_simulation::models::Balances,
        ) -> std::result::Result<(), tycho_simulation::protocol::errors::TransitionError<String>> {
            Ok(())
        }

        fn as_any(&self) -> &(dyn std::any::Any + 'static) {
            self
        }

        fn as_any_mut(&mut self) -> &mut (dyn std::any::Any + 'static) {
            self
        }

        fn eq(&self, other: &(dyn ProtocolSim + 'static)) -> bool {
            other.as_any().downcast_ref::<MockProtocolSim>()
                .map(|other| (self.multiplier - other.multiplier).abs() < f64::EPSILON)
                .unwrap_or(false)
        }
    }

    fn create_mock_swap(multiplier: f64) -> Swap {
        let token_a = Bytes::from_str("0x0001").unwrap();
        let token_b = Bytes::from_str("0x0002").unwrap();
        let pool_addr = Bytes::from_str("0x1001").unwrap();

        let pool_comp = ProtocolComponent {
            id: pool_addr.clone(),
            address: pool_addr.clone(),
            protocol_system: "test".to_string(),
            protocol_type_name: "test_pool".to_string(),
            chain: tycho_common::models::Chain::Ethereum,
            tokens: vec![
                tycho_simulation::models::Token {
                    address: token_a.clone(),
                    symbol: "TOKEN_A".to_string(),
                    decimals: 18,
                    gas: BigUint::from(0u32),
                },
                tycho_simulation::models::Token {
                    address: token_b.clone(),
                    symbol: "TOKEN_B".to_string(),
                    decimals: 18,
                    gas: BigUint::from(0u32),
                },
            ],
            contract_ids: vec![pool_addr.clone()],
            static_attributes: HashMap::new(),
            created_at: chrono::NaiveDateTime::from_timestamp_opt(0, 0).unwrap(),
            creation_tx: tycho_common::Bytes::default(),
        };

        Swap {
            pool_comp,
            pool_sim: Box::new(MockProtocolSim::new(multiplier)),
            zero_for_one: true,
        }
    }

    #[test]
    fn test_path_executor_profitable_path() {
        let swap = create_mock_swap(1.1); // 10% profit per swap
        let path = Path(vec![swap]);
        let executor = PathExecutor::new();

        let result = executor.execute_with_amount(&path, BigUint::from(1000u32));
        assert!(result.is_ok());

        let path_ext = result.unwrap();
        assert_eq!(path_ext.len(), 1);
        assert!(path_ext.is_profitable().unwrap());

        let profit = path_ext.profit().unwrap();
        assert!(profit > BigInt::from(0));
    }

    #[test]
    fn test_path_executor_unprofitable_path() {
        let swap = create_mock_swap(0.9); // 10% loss per swap
        let path = Path(vec![swap]);
        let executor = PathExecutor::new();

        let result = executor.execute_with_amount(&path, BigUint::from(1000u32));
        assert!(result.is_ok());

        let path_ext = result.unwrap();
        assert!(!path_ext.is_profitable().unwrap());

        let profit = path_ext.profit().unwrap();
        assert!(profit < BigInt::from(0));
    }

    #[test]
    fn test_profit_calculator_percentage() {
        let swap = create_mock_swap(1.2); // 20% profit
        let path = Path(vec![swap]);
        let executor = PathExecutor::new();

        let path_ext = executor.execute_with_amount(&path, BigUint::from(1000u32)).unwrap();
        let profit_pct = ProfitCalculator::calculate_profit_percentage(&path_ext).unwrap();

        // Should be approximately 20% profit
        assert!((profit_pct - 0.2).abs() < 0.01);
    }

    #[test]
    fn test_execution_metrics() {
        let swap = create_mock_swap(1.1);
        let path = Path(vec![swap]);
        let executor = PathExecutor::new().with_metrics();

        let path_ext = executor.execute_with_amount(&path, BigUint::from(1000u32)).unwrap();
        let metrics = ExecutionMetrics::from_path_ext(&path_ext).unwrap();

        assert_eq!(metrics.swap_count, 1);
        assert!(metrics.is_profitable);
        assert_eq!(metrics.initial_amount, BigUint::from(1000u32));
        assert!(metrics.final_amount > BigUint::from(1000u32));
    }

    #[test]
    fn test_empty_path_execution() {
        let path = Path(vec![]);
        let executor = PathExecutor::new();

        let result = executor.execute_with_amount(&path, BigUint::from(1000u32));
        assert!(result.is_err());
    }
}
