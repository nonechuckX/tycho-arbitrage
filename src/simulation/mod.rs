//! Simulation engine for atomic arbitrage transactions.
//! 
//! This module provides simulation capabilities for testing arbitrage strategies:
//! - `Simulator`: Core simulation engine
//! - `SimulationResult`: Results from running simulations
//! - Transaction building and payload construction

pub mod encoding;
pub mod parsing;

// Re-export encoding functions for convenience
pub use encoding::{encode_solution, sign_permit, build_solution};

// Re-export parsing types for convenience
pub use parsing::{DecodedSwap, DecodedLogs, LogParser};

use crate::path::PathExt;
use crate::errors::{SimulationError, Result};
use crate::simulation::encoding::{
    create_approval_calldata, encode_router_call, convert_biguint_to_u256
};
use alloy::{
    network::Ethereum,
    primitives::{Address, TxKind, U256},
    providers::{Provider, RootProvider},
    rpc::types::{
        simulate::{SimBlock, SimulatePayload, SimulatedBlock},
        TransactionInput, TransactionRequest,
    },
    signers::local::PrivateKeySigner,
};
use num_bigint::BigUint;
use std::sync::Arc;
use tycho_common::Bytes;
use tycho_execution::encoding::models::Swap as TychoExecutionSwap;

/// Result of running a simulation, containing transaction requests and simulation data.
#[derive(Debug)]
pub struct SimulationResult {
    pub approval_request: TransactionRequest,
    pub swap_request: TransactionRequest,
    pub simulated_blocks: Vec<SimulatedBlock>,
}

/// Core simulation engine for arbitrage transactions.
pub struct Simulator {
    chain_id: u64,
    permit2_address: Address,
}

impl Simulator {
    /// Create a new simulator from an ArbitrageConfig.
    /// 
    /// # Arguments
    /// 
    /// * `config` - The arbitrage configuration containing chain and permit2 settings
    pub fn from_config(config: &crate::config::ArbitrageConfig) -> Self {
        Self {
            chain_id: config.chain_id,
            permit2_address: config.permit2_address,
        }
    }


    /// Run a simulation for the given path and parameters.
    /// 
    /// This method builds the necessary transactions, creates a simulation payload,
    /// and executes the simulation using the provided RPC provider.
    /// 
    /// # Arguments
    /// 
    /// * `provider` - The RPC provider for simulation
    /// * `path` - The executed trading path to simulate
    /// * `nonce` - The account nonce to use
    /// * `base_fee` - The base fee for the block
    /// * `signer` - The signer for creating transactions
    /// 
    /// # Returns
    /// 
    /// A `SimulationResult` containing the transaction requests and simulation data.
    pub async fn run_simulation(
        &self,
        provider: &Arc<RootProvider<Ethereum>>,
        path: &PathExt,
        nonce: u64,
        base_fee: U256,
        signer: &PrivateKeySigner,
    ) -> Result<SimulationResult> {
        let start_time = std::time::Instant::now();
        
        tracing::debug!(
            path_length = path.len(),
            nonce = nonce,
            base_fee = %base_fee,
            signer_address = %signer.address(),
            "Starting simulation"
        );

        let (approval_request, swap_request) =
            self.build_transaction_requests(path, nonce, base_fee, signer)?;

        tracing::debug!(
            approval_gas = approval_request.gas,
            swap_gas = swap_request.gas,
            "Transaction requests built"
        );

        let payload = self.build_simulation_payload(approval_request.clone(), swap_request.clone());
        
        let simulation_start = std::time::Instant::now();
        let simulation_result = provider.simulate(&payload).await;
        let simulation_duration = simulation_start.elapsed();

        match simulation_result {
            Ok(simulated_blocks) => {
                let total_duration = start_time.elapsed();
                
                tracing::info!(
                    path_length = path.len(),
                    simulation_duration_ms = simulation_duration.as_millis(),
                    total_duration_ms = total_duration.as_millis(),
                    blocks_simulated = simulated_blocks.len(),
                    "Simulation completed successfully"
                );

                // Log gas usage if available
                if let Some(first_block) = simulated_blocks.first() {
                    let total_gas_used: u64 = first_block.calls.iter().map(|call| call.gas_used).sum();
                    tracing::debug!(
                        total_gas_used = total_gas_used,
                        call_count = first_block.calls.len(),
                        "Simulation gas usage"
                    );
                }

                Ok(SimulationResult {
                    approval_request,
                    swap_request,
                    simulated_blocks,
                })
            }
            Err(e) => {
                let total_duration = start_time.elapsed();
                
                tracing::error!(
                    error = %e,
                    path_length = path.len(),
                    simulation_duration_ms = simulation_duration.as_millis(),
                    total_duration_ms = total_duration.as_millis(),
                    "Simulation failed"
                );
                
                Err(e.into())
            }
        }
    }

    /// Build the transaction requests needed for the simulation.
    fn build_transaction_requests(
        &self,
        path: &PathExt,
        nonce: u64,
        base_fee: U256,
        signer: &PrivateKeySigner,
    ) -> Result<(TransactionRequest, TransactionRequest)> {
        let tycho_swaps = self.extract_tycho_swaps(path);
        let first_swap = path.first()
            .ok_or_else(|| SimulationError::SimulationFailed { 
                reason: "Empty path: no swaps available".to_string() 
            })?;
        
        let amt_in = &first_swap.amount_in;
        let start_token = Address::from_slice(first_swap.token_in().address.as_ref());

        let (router_calldata, router_address) =
            self.extract_router_details(tycho_swaps, amt_in.clone(), signer, path)?;
        let amount_in_u256 = convert_biguint_to_u256(amt_in)?;

        let approval_request =
            self.create_approval_request(&start_token, &amount_in_u256, nonce, base_fee, signer)?;
        let swap_request =
            self.create_swap_request(&router_address, router_calldata, nonce + 1, base_fee, signer)?;

        Ok((approval_request, swap_request))
    }

    /// Build the simulation payload from transaction requests.
    fn build_simulation_payload(
        &self,
        approval_request: TransactionRequest,
        swap_request: TransactionRequest,
    ) -> SimulatePayload {
        SimulatePayload {
            block_state_calls: vec![SimBlock {
                block_overrides: None,
                state_overrides: None,
                calls: vec![approval_request, swap_request],
            }],
            trace_transfers: true,
            validation: true,
            return_full_transactions: true,
        }
    }

    /// Extract Tycho execution swaps from the path.
    fn extract_tycho_swaps(&self, path: &PathExt) -> Vec<TychoExecutionSwap> {
        let mut swaps = Vec::with_capacity(path.len());
        for swap in path.iter() {
            swaps.push(TychoExecutionSwap {
                component: swap.pool_comp.clone().into(),
                token_in: swap.token_in().address.clone(),
                token_out: swap.token_out().address.clone(),
                split: 0.0,
            });
        }
        swaps
    }

    /// Extract router details from the swaps and build the solution.
    fn extract_router_details(
        &self,
        swaps: Vec<TychoExecutionSwap>,
        amt_in: BigUint,
        signer: &PrivateKeySigner,
        path: &PathExt,
    ) -> Result<(alloy::primitives::Bytes, Address)> {
        let sender_address = Bytes::from(signer.address().as_slice());
        
        // Get the expected final output amount from the last swap in the path
        let expected_amount_out = path.last()
            .ok_or_else(|| SimulationError::SimulationFailed {
                reason: "Empty path: no swaps available for amount calculation".to_string()
            })?
            .amount_out.clone();
        
        let solution = build_solution(&swaps, amt_in, &sender_address, expected_amount_out)?;
        let chain = crate::utils::chain_name(self.chain_id)?;
        let encoded_solution = encode_solution(&solution, chain)?;

        let router_address = Address::from_slice(encoded_solution.interacting_with.as_ref());
        
        // Sign the permit
        let permit = encoded_solution
            .permit
            .as_ref()
            .ok_or(SimulationError::InvalidSimulationPayload)?;
        let permit_signature = sign_permit(permit, signer, self.chain_id, self.permit2_address)?;
        
        let amount_in_u256 = convert_biguint_to_u256(&solution.given_amount)?;
        let router_calldata = encode_router_call(
            &encoded_solution,
            &amount_in_u256,
            &solution,
            &permit_signature,
        )?;

        Ok((router_calldata, router_address))
    }

    /// Create an approval transaction request.
    fn create_approval_request(
        &self,
        start_token: &Address,
        amount_in: &U256,
        nonce: u64,
        base_fee: U256,
        signer: &PrivateKeySigner,
    ) -> Result<TransactionRequest> {
        let approve_calldata = create_approval_calldata(self.permit2_address, *amount_in);

        Ok(TransactionRequest {
            from: Some(signer.address()),
            to: Some(TxKind::Call(*start_token)),
            input: TransactionInput {
                input: Some(approve_calldata),
                data: None,
            },
            gas: Some(100_000),
            max_fee_per_gas: Some((base_fee * U256::from(10) / U256::from(7)).to::<u128>()),
            max_priority_fee_per_gas: Some(0u128),
            chain_id: Some(self.chain_id),
            nonce: Some(nonce),
            ..Default::default()
        })
    }

    /// Create a swap transaction request.
    fn create_swap_request(
        &self,
        router_address: &Address,
        router_calldata: alloy::primitives::Bytes,
        nonce: u64,
        base_fee: U256,
        signer: &PrivateKeySigner,
    ) -> Result<TransactionRequest> {
        Ok(TransactionRequest {
            from: Some(signer.address()),
            to: Some(TxKind::Call(*router_address)),
            input: TransactionInput {
                input: Some(router_calldata),
                data: None,
            },
            gas: Some(1_000_000),
            max_fee_per_gas: Some((base_fee * U256::from(10) / U256::from(7)).to::<u128>()),
            max_priority_fee_per_gas: None,
            chain_id: Some(self.chain_id),
            nonce: Some(nonce),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ArbitrageConfig;

    #[test]
    fn test_simulator_creation() {
        let config = ArbitrageConfig::from_env("ethereum").unwrap();
        let simulator = Simulator::from_config(&config);
        assert_eq!(simulator.chain_id, 1);
    }

    #[test]
    fn test_simulator_invalid_chain() {
        let result = ArbitrageConfig::from_env("invalid_chain");
        assert!(result.is_err());
    }
}
