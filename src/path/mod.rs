//! Trading path types and operations for atomic arbitrage.
//! 
//! This module provides comprehensive path functionality for arbitrage trading,
//! organized into focused sub-modules for better maintainability and clarity.

pub mod creation;
pub mod execution;
pub mod optimization;
pub mod repository;
pub mod swap;

// Re-export types for convenience
pub use creation::{PathBuilder, PathValidator};
pub use execution::{PathExecutor, ProfitCalculator, ExecutionMetrics};
pub use optimization::{PathOptimizer, OptimizationResult};
pub use repository::{PathRepository, RepositoryStatistics};
pub use swap::{Swap, SwapExt, SwapForStorage};

use crate::errors::{PathError, Result};
use num_bigint::{BigInt, BigUint, Sign};
use std::{fmt, iter::FromIterator, ops::Deref};
use tycho_common::Bytes;

/// A trading path consisting of a sequence of swaps.
#[derive(Clone)]
pub struct Path(pub Vec<Swap>);

impl Deref for Path {
    type Target = Vec<Swap>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromIterator<Swap> for Path {
    fn from_iter<I: IntoIterator<Item = Swap>>(iter: I) -> Self {
        Path(iter.into_iter().collect())
    }
}

impl Path {
    /// Get the starting token address for this path.
    pub fn start_token(&self) -> Result<Bytes> {
        let first_swap = self.first()
            .ok_or_else(|| PathError::EmptyPath)?;
        
        Ok(if first_swap.zero_for_one {
            first_swap.pool_comp.tokens[0].address.clone()
        } else {
            first_swap.pool_comp.tokens[1].address.clone()
        })
    }

    /// Get the number of swaps in this path.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Calculate the product of spot prices along the path.
    pub fn spot_price_product(&self) -> Result<f64> {
        let mut product = 1.0;

        for swap in self.iter() {
            product *= swap.spot_price()?;
        }

        Ok(product)
    }

    /// Calculate the profit/loss for a given input amount.
    /// 
    /// Returns the difference between output and input amounts.
    /// Positive values indicate profit, negative values indicate loss.
    pub fn calculate_profit_loss(&self, amount_in: BigUint) -> Result<BigInt> {
        if self.is_empty() {
            return Err(PathError::EmptyPath.into());
        }

        let mut current_amount = amount_in.clone();

        for swap in self.iter() {
            let (max_in, max_out) = swap.get_limits()?;

            if max_in < current_amount {
                return Err(PathError::AmountExceedsLimits { 
                    requested: current_amount.to_string(), 
                    max_available: max_in.to_string() 
                }.into());
            }

            let res = swap.get_amount_out(current_amount)?;
            current_amount = res.amount;

            if max_out < current_amount {
                return Err(PathError::AmountExceedsLimits { 
                    requested: current_amount.to_string(), 
                    max_available: max_out.to_string() 
                }.into());
            }
        }

        let amt_in = BigInt::from_biguint(Sign::Plus, amount_in);
        let amt_out = BigInt::from_biguint(Sign::Plus, current_amount);
        let profit = amt_out - amt_in;

        Ok(profit)
    }

    /// Execute the path with a specific input amount to get detailed results.
    pub fn execute_with_amount(&self, amount_in: BigUint) -> Result<PathExt> {
        if self.is_empty() {
            return Err(PathError::EmptyPath.into());
        }

        let mut current_amount = amount_in.clone();
        let mut swaps = Vec::with_capacity(self.len());

        for swap in self.iter() {
            let amount_for_swap = current_amount.clone();
            let res = swap.get_amount_out(current_amount)?;
            let swap_ext = SwapExt {
                pool_comp: swap.pool_comp.clone(),
                pool_sim: swap.pool_sim.clone(),
                zero_for_one: swap.zero_for_one,
                amount_in: amount_for_swap,
                amount_out: res.amount.clone(),
                gas: res.gas,
            };
            current_amount = res.amount;
            swaps.push(swap_ext);
        }

        Ok(PathExt(swaps))
    }
}

impl fmt::Debug for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let start_token = self.start_token().ok();
        let pools: Vec<_> = self.iter().map(|s| &s.pool_comp.id).collect();
        let protocols: Vec<_> = self.iter().map(|s| &s.pool_comp.protocol_system).collect();
        
        f.debug_struct("Path")
            .field("length", &self.len())
            .field("start_token", &start_token)
            .field("pools", &pools)
            .field("protocols", &protocols)
            .finish()
    }
}

/// An executed trading path with specific amounts and gas costs.
#[derive(Clone)]
pub struct PathExt(pub Vec<SwapExt>);

impl Deref for PathExt {
    type Target = Vec<SwapExt>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromIterator<SwapExt> for PathExt {
    fn from_iter<I: IntoIterator<Item = SwapExt>>(iter: I) -> Self {
        PathExt(iter.into_iter().collect())
    }
}

impl PathExt {
    /// Check if this executed path is profitable.
    pub fn is_profitable(&self) -> Result<bool> {
        let last_swap = self.last()
            .ok_or_else(|| PathError::EmptyPath)?;
        let first_swap = self.first()
            .ok_or_else(|| PathError::EmptyPath)?;
        
        Ok(last_swap.amount_out > first_swap.amount_in)
    }

    /// Calculate the profit from this executed path.
    /// 
    /// Returns the difference between the final output amount and initial input amount.
    /// Positive values indicate profit, negative values indicate loss.
    pub fn profit(&self) -> Result<BigInt> {
        let last_swap = self.last()
            .ok_or_else(|| PathError::EmptyPath)?;
        let first_swap = self.first()
            .ok_or_else(|| PathError::EmptyPath)?;
        
        // Safe subtraction using BigInt to handle negative results (losses)
        let amount_out = BigInt::from(last_swap.amount_out.clone());
        let amount_in = BigInt::from(first_swap.amount_in.clone());
        
        Ok(amount_out - amount_in)
    }

    /// Get the starting token address for this executed path.
    pub fn start_token(&self) -> Result<Bytes> {
        let first_swap = self.first()
            .ok_or_else(|| PathError::EmptyPath)?;
        
        Ok(if first_swap.zero_for_one {
            first_swap.pool_comp.tokens[0].address.clone()
        } else {
            first_swap.pool_comp.tokens[1].address.clone()
        })
    }
}

impl fmt::Debug for PathExt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let start_token = self.start_token().ok();
        let total_gas: BigUint = self.iter().map(|s| &s.gas).sum();
        let profit = self.profit().ok();
        let is_profitable = self.is_profitable().ok();
        
        f.debug_struct("PathExt")
            .field("length", &self.len())
            .field("start_token", &start_token)
            .field("input_amount", &self.first().map(|s| &s.amount_in))
            .field("output_amount", &self.last().map(|s| &s.amount_out))
            .field("profit", &profit)
            .field("is_profitable", &is_profitable)
            .field("total_gas", &total_gas)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigUint;

    #[test]
    fn test_path_basic_operations() {
        // Create a simple path with mock swaps for testing basic operations
        let path = Path(vec![]);
        
        // Test empty path
        assert_eq!(path.len(), 0);
        assert!(path.start_token().is_err());
        
        // Empty path should return an error for profit calculation
        let profit_result = path.calculate_profit_loss(BigUint::from(1000u32));
        assert!(profit_result.is_err());
        
        // Empty path should return an error for execution
        let execution_result = path.execute_with_amount(BigUint::from(1000u32));
        assert!(execution_result.is_err());
    }

    #[test]
    fn test_path_ext_basic_operations() {
        // Test empty PathExt
        let path_ext = PathExt(vec![]);
        
        assert_eq!(path_ext.len(), 0);
        assert!(path_ext.is_profitable().is_err());
        assert!(path_ext.profit().is_err());
        assert!(path_ext.start_token().is_err());
    }
}
