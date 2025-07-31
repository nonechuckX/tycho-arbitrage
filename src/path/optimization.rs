//! Path optimization trait and result types for atomic arbitrage.
//!
//! This module provides the core trait and types for path optimization, allowing
//! users to implement their own optimization strategies. The concrete optimizer
//! implementations have been moved to the examples to demonstrate different approaches.
//!
//! # Example Optimizers
//!
//! See the `examples/atomic/context/optimizers.rs` file for complete implementations of:
//! - Ternary Search Optimizer
//! - Golden Section Search Optimizer  
//! - Grid Search Optimizer
//!
//! These can serve as starting points for your own optimization strategies.

use crate::errors::Result;
use crate::path::{Path, PathExt};
use num_bigint::{BigInt, BigUint};
use std::fmt;

/// Result of a path optimization operation.
#[derive(Debug, Clone)]
pub struct OptimizationResult {
    /// The optimal input amount found
    pub optimal_amount: BigUint,
    /// The expected profit at the optimal amount
    pub expected_profit: BigInt,
    /// The number of iterations performed during optimization
    pub iterations: usize,
    /// Whether the optimization converged successfully
    pub converged: bool,
    /// The final tolerance achieved
    pub final_tolerance: f64,
}

impl OptimizationResult {
    /// Create a new optimization result.
    pub fn new(
        optimal_amount: BigUint,
        expected_profit: BigInt,
        iterations: usize,
        converged: bool,
        final_tolerance: f64,
    ) -> Self {
        Self {
            optimal_amount,
            expected_profit,
            iterations,
            converged,
            final_tolerance,
        }
    }

    /// Check if the optimization found a profitable solution.
    pub fn is_profitable(&self) -> bool {
        self.expected_profit > BigInt::from(0)
    }
}

impl fmt::Display for OptimizationResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "OptimizationResult {{ amount: {}, profit: {}, iterations: {}, converged: {} }}",
            self.optimal_amount, self.expected_profit, self.iterations, self.converged
        )
    }
}

/// Trait for path optimization strategies.
///
/// This trait allows for different optimization algorithms to be used
/// interchangeably for finding optimal input amounts. Users should implement
/// this trait to create custom optimization strategies.
///
/// # Required Methods
///
/// - `find_optimal_amount`: Find the optimal input amount for a given path
///
/// # Optional Methods
///
/// - `optimize_and_execute`: Find optimal amount and execute the path (default implementation provided)
///
pub trait PathOptimizer {
    /// Find the optimal input amount for a given path.
    ///
    /// This method should analyze the path and determine the input amount
    /// that maximizes profit. The implementation can use any optimization
    /// algorithm suitable for the specific use case.
    ///
    /// # Arguments
    ///
    /// * `path` - The trading path to optimize
    ///
    /// # Returns
    ///
    /// An `OptimizationResult` containing the optimal amount and expected profit
    ///
    /// # Errors
    ///
    /// Should return an error if:
    /// - The path is empty or invalid
    /// - The optimization algorithm fails to converge
    /// - Any path evaluation fails during optimization
    fn find_optimal_amount(&self, path: &Path) -> Result<OptimizationResult>;

    /// Find the optimal input amount and execute the path.
    ///
    /// This is a convenience method that combines optimization with execution.
    /// The default implementation calls `find_optimal_amount` and then executes
    /// the path with the optimal amount.
    ///
    /// # Arguments
    ///
    /// * `path` - The trading path to optimize and execute
    ///
    /// # Returns
    ///
    /// A tuple containing the optimization result and executed path
    ///
    /// # Errors
    ///
    /// Returns an error if optimization or execution fails
    fn optimize_and_execute(&self, path: &Path) -> Result<(OptimizationResult, PathExt)> {
        let optimization_result = self.find_optimal_amount(path)?;
        let executed_path = path.execute_with_amount(optimization_result.optimal_amount.clone())?;
        Ok((optimization_result, executed_path))
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

    // Simple test optimizer
    struct TestOptimizer {
        test_amount: BigUint,
    }

    impl TestOptimizer {
        fn new(test_amount: BigUint) -> Self {
            Self { test_amount }
        }
    }

    impl PathOptimizer for TestOptimizer {
        fn find_optimal_amount(&self, path: &Path) -> Result<OptimizationResult> {
            let profit = path.calculate_profit_loss(self.test_amount.clone())?;
            Ok(OptimizationResult::new(
                self.test_amount.clone(),
                profit,
                1,
                true,
                0.0,
            ))
        }
    }

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
            Ok((BigUint::from(10_000_000u32), BigUint::from(10_000_000u32)))
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

    fn create_mock_path() -> Path {
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

        let swap = Swap {
            pool_comp,
            pool_sim: Box::new(MockProtocolSim::new(1.1)),
            zero_for_one: true,
        };

        Path(vec![swap])
    }

    #[test]
    fn test_optimization_result() {
        let result = OptimizationResult::new(
            BigUint::from(1000u32),
            BigInt::from(100),
            10,
            true,
            0.001,
        );

        assert_eq!(result.optimal_amount, BigUint::from(1000u32));
        assert_eq!(result.expected_profit, BigInt::from(100));
        assert_eq!(result.iterations, 10);
        assert!(result.converged);
        assert_eq!(result.final_tolerance, 0.001);
        assert!(result.is_profitable());
    }

    #[test]
    fn test_path_optimizer_trait() {
        let path = create_mock_path();
        let optimizer = TestOptimizer::new(BigUint::from(1000u32));

        let result = optimizer.find_optimal_amount(&path);
        assert!(result.is_ok());

        let optimization_result = result.unwrap();
        assert_eq!(optimization_result.optimal_amount, BigUint::from(1000u32));
        assert!(optimization_result.is_profitable());
    }

    #[test]
    fn test_optimize_and_execute() {
        let path = create_mock_path();
        let optimizer = TestOptimizer::new(BigUint::from(1000u32));

        let result = optimizer.optimize_and_execute(&path);
        assert!(result.is_ok());

        let (optimization_result, path_ext) = result.unwrap();
        assert!(optimization_result.is_profitable());
        assert_eq!(path_ext.len(), 1);
        assert!(path_ext.is_profitable().unwrap());
    }
}
