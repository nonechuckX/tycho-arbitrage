//! Example optimizer implementations for atomic arbitrage paths.
//!
//! This module provides concrete implementations of the `PathOptimizer` trait
//! to demonstrate different optimization strategies. Users can use these as
//! starting points for their own optimization algorithms.
//!
//! # Available Optimizers
//!
//! - **`TernarySearchOptimizer`**: Uses ternary search to find optimal amounts
//! - **`GoldenSectionOptimizer`**: Uses golden section search for optimization
//! - **`GridSearchOptimizer`**: Simple grid search for comparison and testing
//!
//! # Usage
//!
//! ```rust,no_run
//! use tycho_atomic_arbitrage::path::optimization::PathOptimizer;
//! use crate::optimizers::TernarySearchOptimizer;
//! 
//! let optimizer = TernarySearchOptimizer::new()
//!     .with_max_iterations(100)
//!     .with_tolerance(1e-6);
//! 
//! let result = optimizer.find_optimal_amount(&path)?;
//! ```

use tycho_atomic_arbitrage::path::optimization::{PathOptimizer, OptimizationResult};
use tycho_atomic_arbitrage::path::Path;
use tycho_atomic_arbitrage::errors::{PathError, Result};
use num_bigint::{BigInt, BigUint};

/// Ternary search-based path optimizer.
///
/// Uses ternary search to find the optimal input amount by evaluating the profit
/// function at different points and narrowing down the search space.
pub struct TernarySearchOptimizer {
    /// Maximum number of iterations
    max_iterations: usize,
    /// Convergence tolerance
    tolerance: f64,
    /// Minimum search amount
    min_amount: BigUint,
    /// Maximum search amount
    max_amount: BigUint,
}

impl TernarySearchOptimizer {
    /// Create a new ternary search optimizer with default parameters.
    pub fn new() -> Self {
        Self {
            max_iterations: 100,
            tolerance: 1e-6,
            min_amount: BigUint::from(1u32),
            max_amount: BigUint::from(1_000_000_000u64), // 1B units
        }
    }

    /// Set the maximum number of iterations.
    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    /// Set the convergence tolerance.
    pub fn with_tolerance(mut self, tolerance: f64) -> Self {
        self.tolerance = tolerance;
        self
    }

    /// Set the search range.
    pub fn with_search_range(mut self, min_amount: BigUint, max_amount: BigUint) -> Self {
        self.min_amount = min_amount;
        self.max_amount = max_amount;
        self
    }

    /// Convert BigUint to f64 for calculations.
    fn biguint_to_f64(&self, value: &BigUint) -> f64 {
        value.to_string().parse().unwrap_or(0.0)
    }

    /// Convert f64 to BigUint for calculations.
    fn f64_to_biguint(&self, value: f64) -> BigUint {
        if value <= 0.0 {
            BigUint::from(0u32)
        } else {
            BigUint::from(value as u64)
        }
    }

    /// Evaluate the profit function at a given amount.
    fn evaluate_profit(&self, path: &Path, amount: &BigUint) -> BigInt {
        path.calculate_profit_loss(amount.clone()).unwrap_or(BigInt::from(0))
    }
}

impl Default for TernarySearchOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

impl PathOptimizer for TernarySearchOptimizer {
    fn find_optimal_amount(&self, path: &Path) -> Result<OptimizationResult> {
        if path.is_empty() {
            return Err(PathError::EmptyPath.into());
        }

        tracing::debug!(
            path_length = path.len(),
            max_iterations = self.max_iterations,
            tolerance = self.tolerance,
            "Starting ternary search optimization"
        );

        let mut left = self.biguint_to_f64(&self.min_amount);
        let mut right = self.biguint_to_f64(&self.max_amount);
        let mut iterations = 0;
        let mut best_amount = self.min_amount.clone();
        let mut best_profit = BigInt::from(0);

        while iterations < self.max_iterations && (right - left) > self.tolerance {
            let mid1 = left + (right - left) / 3.0;
            let mid2 = right - (right - left) / 3.0;

            let amount1 = self.f64_to_biguint(mid1);
            let amount2 = self.f64_to_biguint(mid2);

            let profit1 = self.evaluate_profit(path, &amount1);
            let profit2 = self.evaluate_profit(path, &amount2);

            // Update best result
            if profit1 > best_profit {
                best_profit = profit1.clone();
                best_amount = amount1.clone();
            }
            if profit2 > best_profit {
                best_profit = profit2.clone();
                best_amount = amount2.clone();
            }

            // Narrow search space
            if profit1 > profit2 {
                right = mid2;
            } else {
                left = mid1;
            }

            iterations += 1;

            tracing::trace!(
                iteration = iterations,
                left = left,
                right = right,
                mid1 = mid1,
                mid2 = mid2,
                profit1 = %profit1,
                profit2 = %profit2,
                "Ternary search iteration"
            );
        }

        let converged = (right - left) <= self.tolerance;
        let final_tolerance = right - left;

        let result = OptimizationResult::new(
            best_amount,
            best_profit,
            iterations,
            converged,
            final_tolerance,
        );

        tracing::debug!(
            optimal_amount = %result.optimal_amount,
            expected_profit = %result.expected_profit,
            iterations = result.iterations,
            converged = result.converged,
            "Ternary search optimization completed"
        );

        Ok(result)
    }
}

/// Golden section search-based path optimizer.
///
/// Uses the golden section search algorithm to find the optimal input amount.
/// This method is often more efficient than ternary search for unimodal functions.
pub struct GoldenSectionOptimizer {
    /// Maximum number of iterations
    max_iterations: usize,
    /// Convergence tolerance
    tolerance: f64,
    /// Minimum search amount
    min_amount: BigUint,
    /// Maximum search amount
    max_amount: BigUint,
    /// Golden ratio constant
    golden_ratio: f64,
}

impl GoldenSectionOptimizer {
    /// Create a new golden section optimizer with default parameters.
    pub fn new() -> Self {
        Self {
            max_iterations: 100,
            tolerance: 1e-6,
            min_amount: BigUint::from(1u32),
            max_amount: BigUint::from(1_000_000_000u64),
            golden_ratio: (1.0 + 5.0_f64.sqrt()) / 2.0, // φ ≈ 1.618
        }
    }

    /// Set the maximum number of iterations.
    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    /// Set the convergence tolerance.
    pub fn with_tolerance(mut self, tolerance: f64) -> Self {
        self.tolerance = tolerance;
        self
    }

    /// Set the search range.
    pub fn with_search_range(mut self, min_amount: BigUint, max_amount: BigUint) -> Self {
        self.min_amount = min_amount;
        self.max_amount = max_amount;
        self
    }

    /// Convert BigUint to f64 for calculations.
    fn biguint_to_f64(&self, value: &BigUint) -> f64 {
        value.to_string().parse().unwrap_or(0.0)
    }

    /// Convert f64 to BigUint for calculations.
    fn f64_to_biguint(&self, value: f64) -> BigUint {
        if value <= 0.0 {
            BigUint::from(0u32)
        } else {
            BigUint::from(value as u64)
        }
    }

    /// Evaluate the profit function at a given amount.
    fn evaluate_profit(&self, path: &Path, amount: &BigUint) -> BigInt {
        path.calculate_profit_loss(amount.clone()).unwrap_or(BigInt::from(0))
    }
}

impl Default for GoldenSectionOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

impl PathOptimizer for GoldenSectionOptimizer {
    fn find_optimal_amount(&self, path: &Path) -> Result<OptimizationResult> {
        if path.is_empty() {
            return Err(PathError::EmptyPath.into());
        }

        tracing::debug!(
            path_length = path.len(),
            max_iterations = self.max_iterations,
            tolerance = self.tolerance,
            "Starting golden section search optimization"
        );

        let mut a = self.biguint_to_f64(&self.min_amount);
        let mut b = self.biguint_to_f64(&self.max_amount);
        let mut iterations = 0;
        let mut best_amount = self.min_amount.clone();
        let mut best_profit = BigInt::from(0);

        // Initial points
        let mut c = b - (b - a) / self.golden_ratio;
        let mut d = a + (b - a) / self.golden_ratio;

        let mut fc = self.evaluate_profit(path, &self.f64_to_biguint(c));
        let mut fd = self.evaluate_profit(path, &self.f64_to_biguint(d));

        while iterations < self.max_iterations && (b - a).abs() > self.tolerance {
            // Update best result
            let amount_c = self.f64_to_biguint(c);
            let amount_d = self.f64_to_biguint(d);

            if fc > best_profit {
                best_profit = fc.clone();
                best_amount = amount_c.clone();
            }
            if fd > best_profit {
                best_profit = fd.clone();
                best_amount = amount_d.clone();
            }

            if fc > fd {
                b = d;
                d = c;
                fd = fc;
                c = b - (b - a) / self.golden_ratio;
                fc = self.evaluate_profit(path, &self.f64_to_biguint(c));
            } else {
                a = c;
                c = d;
                fc = fd;
                d = a + (b - a) / self.golden_ratio;
                fd = self.evaluate_profit(path, &self.f64_to_biguint(d));
            }

            iterations += 1;

            tracing::trace!(
                iteration = iterations,
                a = a,
                b = b,
                c = c,
                d = d,
                fc = %fc,
                fd = %fd,
                "Golden section search iteration"
            );
        }

        let converged = (b - a).abs() <= self.tolerance;
        let final_tolerance = (b - a).abs();

        let result = OptimizationResult::new(
            best_amount,
            best_profit,
            iterations,
            converged,
            final_tolerance,
        );

        tracing::debug!(
            optimal_amount = %result.optimal_amount,
            expected_profit = %result.expected_profit,
            iterations = result.iterations,
            converged = result.converged,
            "Golden section search optimization completed"
        );

        Ok(result)
    }
}

/// Simple grid search optimizer for comparison and testing.
///
/// Evaluates the profit function at regular intervals across the search space.
/// Less efficient than other methods but useful for validation and debugging.
pub struct GridSearchOptimizer {
    /// Number of grid points to evaluate
    grid_points: usize,
    /// Minimum search amount
    min_amount: BigUint,
    /// Maximum search amount
    max_amount: BigUint,
}

impl GridSearchOptimizer {
    /// Create a new grid search optimizer.
    pub fn new(grid_points: usize) -> Self {
        Self {
            grid_points,
            min_amount: BigUint::from(1u32),
            max_amount: BigUint::from(1_000_000_000u64),
        }
    }

    /// Set the search range.
    pub fn with_search_range(mut self, min_amount: BigUint, max_amount: BigUint) -> Self {
        self.min_amount = min_amount;
        self.max_amount = max_amount;
        self
    }

    /// Convert BigUint to f64 for calculations.
    fn biguint_to_f64(&self, value: &BigUint) -> f64 {
        value.to_string().parse().unwrap_or(0.0)
    }

    /// Convert f64 to BigUint for calculations.
    fn f64_to_biguint(&self, value: f64) -> BigUint {
        if value <= 0.0 {
            BigUint::from(0u32)
        } else {
            BigUint::from(value as u64)
        }
    }
}

impl PathOptimizer for GridSearchOptimizer {
    fn find_optimal_amount(&self, path: &Path) -> Result<OptimizationResult> {
        if path.is_empty() {
            return Err(PathError::EmptyPath.into());
        }

        tracing::debug!(
            path_length = path.len(),
            grid_points = self.grid_points,
            "Starting grid search optimization"
        );

        let min_f64 = self.biguint_to_f64(&self.min_amount);
        let max_f64 = self.biguint_to_f64(&self.max_amount);
        let step = (max_f64 - min_f64) / (self.grid_points - 1) as f64;

        let mut best_amount = self.min_amount.clone();
        let mut best_profit = BigInt::from(0);

        for i in 0..self.grid_points {
            let amount_f64 = min_f64 + i as f64 * step;
            let amount = self.f64_to_biguint(amount_f64);
            
            let profit = path.calculate_profit_loss(amount.clone()).unwrap_or(BigInt::from(0));
            
            if profit > best_profit {
                best_profit = profit;
                best_amount = amount;
            }
        }

        let result = OptimizationResult::new(
            best_amount,
            best_profit,
            self.grid_points,
            true, // Grid search always "converges"
            0.0,
        );

        tracing::debug!(
            optimal_amount = %result.optimal_amount,
            expected_profit = %result.expected_profit,
            grid_points = self.grid_points,
            "Grid search optimization completed"
        );

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tycho_atomic_arbitrage::path::{Path, Swap};
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
            let amount_f64 = amount_in.to_string().parse::<f64>().unwrap_or(0.0);
            
            // Simple quadratic function with maximum at optimal_amount
            if amount_f64 <= 0.0 {
                return Ok(tycho_simulation::protocol::models::GetAmountOutResult {
                    amount: amount_in,
                    gas: BigUint::from(21000u32),
                    new_state: Box::new(self.clone()),
                });
            }
            
            let ratio = amount_f64 / 1000.0; // Optimal at 1000
            let multiplier = if ratio <= 2.0 {
                1.0 + 0.1 * ratio * (2.0 - ratio) // Simple parabola with max at ratio=1
            } else {
                0.9 // Diminishing returns for very large amounts
            };
            
            let amount_out = BigUint::from((amount_f64 * multiplier).max(0.0) as u64);

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
            pool_sim: Box::new(MockProtocolSim::new(1.0)),
            zero_for_one: true,
        };

        Path(vec![swap])
    }

    #[test]
    fn test_ternary_search_optimizer() {
        let path = create_mock_path();
        let optimizer = TernarySearchOptimizer::new()
            .with_max_iterations(50)
            .with_tolerance(1.0);

        let result = optimizer.find_optimal_amount(&path);
        assert!(result.is_ok());

        let optimization_result = result.unwrap();
        assert!(optimization_result.converged);
        assert!(optimization_result.iterations > 0);
    }

    #[test]
    fn test_golden_section_optimizer() {
        let path = create_mock_path();
        let optimizer = GoldenSectionOptimizer::new()
            .with_max_iterations(50)
            .with_tolerance(1.0);

        let result = optimizer.find_optimal_amount(&path);
        assert!(result.is_ok());

        let optimization_result = result.unwrap();
        assert!(optimization_result.converged);
        assert!(optimization_result.iterations > 0);
    }

    #[test]
    fn test_grid_search_optimizer() {
        let path = create_mock_path();
        let optimizer = GridSearchOptimizer::new(100);

        let result = optimizer.find_optimal_amount(&path);
        assert!(result.is_ok());

        let optimization_result = result.unwrap();
        assert!(optimization_result.converged);
        assert_eq!(optimization_result.iterations, 100);
    }

    #[test]
    fn test_optimize_and_execute() {
        let path = create_mock_path();
        let optimizer = TernarySearchOptimizer::new();

        let result = optimizer.optimize_and_execute(&path);
        assert!(result.is_ok());

        let (optimization_result, path_ext) = result.unwrap();
        assert_eq!(path_ext.len(), 1);
    }

    #[test]
    fn test_empty_path_optimization() {
        let path = Path(vec![]);
        let optimizer = TernarySearchOptimizer::new();

        let result = optimizer.find_optimal_amount(&path);
        assert!(result.is_err());
    }
}
