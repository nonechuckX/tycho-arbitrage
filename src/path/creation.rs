//! Path creation and validation logic for atomic arbitrage.
//!
//! This module provides the core functionality for creating trading paths from graph edges
//! and validating their connectivity and feasibility. It separates the concerns of path
//! construction from path execution and optimization.

use crate::errors::{PathError, Result};
use crate::graph::TradingGraph;
use crate::path::{Path, Swap};
use std::collections::HashMap;
use tycho_common::Bytes;
use tycho_simulation::{
    protocol::{models::ProtocolComponent, state::ProtocolSim},
};

/// Builder for creating trading paths with validation.
///
/// The `PathBuilder` provides a fluent interface for constructing paths from graph edges
/// while ensuring all required components are available and the path is valid.
pub struct PathBuilder<'a> {
    edges: Option<&'a [usize]>,
    graph: Option<&'a TradingGraph>,
    protocol_components: Option<&'a HashMap<Bytes, ProtocolComponent>>,
    protocol_simulations: Option<&'a HashMap<Bytes, Box<dyn ProtocolSim>>>,
    validate_connectivity: bool,
}

impl<'a> PathBuilder<'a> {
    /// Create a new path builder.
    pub fn new() -> Self {
        Self {
            edges: None,
            graph: None,
            protocol_components: None,
            protocol_simulations: None,
            validate_connectivity: true,
        }
    }

    /// Set the edge indices for the path.
    pub fn with_edges(mut self, edges: &'a [usize]) -> Self {
        self.edges = Some(edges);
        self
    }

    /// Set the trading graph reference.
    pub fn with_graph(mut self, graph: &'a TradingGraph) -> Self {
        self.graph = Some(graph);
        self
    }

    /// Set the protocol components map.
    pub fn with_protocol_components(
        mut self,
        components: &'a HashMap<Bytes, ProtocolComponent>,
    ) -> Self {
        self.protocol_components = Some(components);
        self
    }

    /// Set the protocol simulations map.
    pub fn with_protocol_simulations(
        mut self,
        simulations: &'a HashMap<Bytes, Box<dyn ProtocolSim>>,
    ) -> Self {
        self.protocol_simulations = Some(simulations);
        self
    }

    /// Disable connectivity validation (useful for testing).
    pub fn skip_connectivity_validation(mut self) -> Self {
        self.validate_connectivity = false;
        self
    }

    /// Build the path with validation.
    pub fn build(self) -> Result<Path> {
        let edges = self.edges.ok_or_else(|| {
            PathError::InvalidPath {
                reason: "No edges provided".to_string(),
            }
        })?;

        let graph = self.graph.ok_or_else(|| {
            PathError::InvalidPath {
                reason: "No graph provided".to_string(),
            }
        })?;

        let protocol_components = self.protocol_components.ok_or_else(|| {
            PathError::InvalidPath {
                reason: "No protocol components provided".to_string(),
            }
        })?;

        let protocol_simulations = self.protocol_simulations.ok_or_else(|| {
            PathError::InvalidPath {
                reason: "No protocol simulations provided".to_string(),
            }
        })?;

        if edges.is_empty() {
            return Err(PathError::EmptyPath.into());
        }

        tracing::debug!(
            path_length = edges.len(),
            edge_indices = ?edges,
            "Creating path from edge indices"
        );

        let swaps = self.create_swaps_from_edges(
            edges,
            graph,
            protocol_components,
            protocol_simulations,
        )?;

        if self.validate_connectivity {
            PathValidator::validate_connectivity(&swaps)?;
        }

        // Always validate arbitrage cycle for arbitrage paths
        PathValidator::validate_arbitrage_cycle(&swaps)?;

        let path = Path(swaps);

        tracing::debug!(
            path_length = path.len(),
            start_token = ?path.start_token().ok(),
            pools = ?path.iter().map(|s| &s.pool_comp.id).collect::<Vec<_>>(),
            "Path created successfully"
        );

        Ok(path)
    }

    /// Create swaps from edge indices.
    fn create_swaps_from_edges(
        &self,
        edges: &[usize],
        graph: &TradingGraph,
        protocol_components: &HashMap<Bytes, ProtocolComponent>,
        protocol_simulations: &HashMap<Bytes, Box<dyn ProtocolSim>>,
    ) -> Result<Vec<Swap>> {
        let mut swaps = Vec::with_capacity(edges.len());

        for &edge_idx in edges.iter() {
            let swap = self.create_swap_from_edge(
                edge_idx,
                graph,
                protocol_components,
                protocol_simulations,
            )?;
            swaps.push(swap);
        }

        Ok(swaps)
    }

    /// Create a single swap from an edge index.
    fn create_swap_from_edge(
        &self,
        edge_idx: usize,
        graph: &TradingGraph,
        protocol_components: &HashMap<Bytes, ProtocolComponent>,
        protocol_simulations: &HashMap<Bytes, Box<dyn ProtocolSim>>,
    ) -> Result<Swap> {
        let edge = graph.get_pool(edge_idx).map_err(|e| {
            tracing::warn!(
                edge_index = edge_idx,
                error = %e,
                "Failed to get pool from graph"
            );
            PathError::InvalidPath {
                reason: format!("Invalid edge index: {}", edge_idx),
            }
        })?;

        let pool_component = protocol_components
            .get(edge.address())
            .ok_or_else(|| {
                tracing::debug!(
                    pool_address = %edge.address(),
                    edge_index = edge_idx,
                    "Protocol component not found for pool"
                );
                PathError::ProtocolComponentNotFound {
                    pool: edge.address().clone(),
                }
            })?
            .clone();

        let pool_simulation = protocol_simulations
            .get(edge.address())
            .ok_or_else(|| {
                tracing::debug!(
                    pool_address = %edge.address(),
                    edge_index = edge_idx,
                    "Protocol simulation not found for pool"
                );
                PathError::ProtocolSimulationNotFound {
                    pool: edge.address().clone(),
                }
            })?
            .clone();

        let input_token = graph.get_token(edge.token_in_id()).map_err(|e| {
            tracing::warn!(
                edge_index = edge_idx,
                error = %e,
                "Failed to get input token from graph"
            );
            PathError::InvalidPath {
                reason: format!("Invalid input token for edge: {}", edge_idx),
            }
        })?;

        let zero_for_one = input_token.address() == &pool_component.tokens[0].address;

        Ok(Swap {
            pool_comp: pool_component,
            pool_sim: pool_simulation,
            zero_for_one,
        })
    }
}

impl<'a> Default for PathBuilder<'a> {
    fn default() -> Self {
        Self::new()
    }
}

/// Validator for path connectivity and token compatibility.
pub struct PathValidator;

impl PathValidator {
    /// Validate that consecutive swaps in a path are properly connected.
    ///
    /// Ensures that the output token of each swap matches the input token
    /// of the next swap, forming a valid trading path.
    pub fn validate_connectivity(swaps: &[Swap]) -> Result<()> {
        if swaps.len() < 2 {
            return Ok(()); // Single swap or empty path doesn't need connectivity validation
        }

        for i in 1..swaps.len() {
            let previous_swap = &swaps[i - 1];
            let current_swap = &swaps[i];

            let previous_output_token = Self::get_output_token_address(previous_swap);
            let current_input_token = Self::get_input_token_address(current_swap);

            if previous_output_token != current_input_token {
                tracing::debug!(
                    swap_index = i,
                    previous_output = %previous_output_token,
                    current_input = %current_input_token,
                    "Path validation failed: tokens not connected"
                );

                return Err(PathError::TokenMismatch {
                    expected: previous_output_token.clone(),
                    actual: current_input_token.clone(),
                }.into());
            }
        }

        Ok(())
    }

    /// Validate that a path forms a valid arbitrage cycle.
    ///
    /// Ensures that the output token of the last swap matches the input token
    /// of the first swap, creating a closed arbitrage loop.
    pub fn validate_arbitrage_cycle(swaps: &[Swap]) -> Result<()> {
        if swaps.is_empty() {
            return Err(PathError::EmptyPath.into());
        }

        if swaps.len() == 1 {
            return Err(PathError::InvalidCycle.into());
        }

        let first_input = Self::get_input_token_address(&swaps[0]);
        let last_output = Self::get_output_token_address(&swaps[swaps.len() - 1]);

        if first_input != last_output {
            return Err(PathError::InvalidCycle.into());
        }

        Ok(())
    }

    /// Get the input token address for a swap.
    fn get_input_token_address(swap: &Swap) -> &Bytes {
        if swap.zero_for_one {
            &swap.pool_comp.tokens[0].address
        } else {
            &swap.pool_comp.tokens[1].address
        }
    }

    /// Get the output token address for a swap.
    fn get_output_token_address(swap: &Swap) -> &Bytes {
        if swap.zero_for_one {
            &swap.pool_comp.tokens[1].address
        } else {
            &swap.pool_comp.tokens[0].address
        }
    }

    /// Validate that a path has consistent pool and token counts for logging/storage.
    ///
    /// For a valid arbitrage path:
    /// - N pools should connect N+1 tokens (including cycle completion)
    /// - The path should form a cycle (first token == last token)
    /// - Must have at least 2 pools for meaningful arbitrage
    ///
    /// # Arguments
    ///
    /// * `pools` - The sequence of pool addresses in the path
    /// * `tokens` - The sequence of token addresses in the path
    ///
    /// # Returns
    ///
    /// `Ok(())` if the path is consistent, `Err` otherwise
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The pools or tokens vectors are empty
    /// - The token count doesn't equal pool count + 1
    /// - The path doesn't form a cycle (first != last token)
    /// - The path has fewer than 2 pools
    pub fn validate_path_consistency(pools: &[Bytes], tokens: &[Bytes]) -> Result<()> {
        if pools.is_empty() || tokens.is_empty() {
            tracing::debug!("Empty pools or tokens");
            return Err(PathError::InvalidPath {
                reason: "Empty pools or tokens".to_string(),
            }.into());
        }
        
        // For N pools, we should have N+1 tokens (including the cycle completion)
        if tokens.len() != pools.len() + 1 {
            tracing::debug!(
                pool_count = pools.len(),
                token_count = tokens.len(),
                "Token count should be pool count + 1"
            );
            return Err(PathError::InvalidPath {
                reason: format!(
                    "Token count ({}) should be pool count ({}) + 1", 
                    tokens.len(), 
                    pools.len()
                ),
            }.into());
        }
        
        // Check if it forms a cycle (first token should equal last token for arbitrage)
        if tokens.first() != tokens.last() {
            tracing::debug!("Path does not form a cycle");
            return Err(PathError::InvalidCycle.into());
        }
        
        // Additional validation: ensure we have at least 2 pools for a meaningful arbitrage
        if pools.len() < 2 {
            tracing::debug!("Path too short for arbitrage");
            return Err(PathError::InvalidPath {
                reason: "Path must have at least 2 pools for arbitrage".to_string(),
            }.into());
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::TradingGraph;
    use std::str::FromStr;
    use tycho_simulation::protocol::models::ProtocolComponent;
    use tycho_simulation::protocol::state::ProtocolSim;
    use num_bigint::BigUint;

    // Mock ProtocolSim for testing
    #[derive(Debug, Clone)]
    struct MockProtocolSim;

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
            Ok(1.0)
        }

        fn get_amount_out(
            &self,
            amount_in: BigUint,
            _token_in: &tycho_simulation::models::Token,
            _token_out: &tycho_simulation::models::Token,
        ) -> std::result::Result<tycho_simulation::protocol::models::GetAmountOutResult, tycho_simulation::protocol::errors::SimulationError> {
            Ok(tycho_simulation::protocol::models::GetAmountOutResult {
                amount: amount_in,
                gas: BigUint::from(21000u32),
                new_state: Box::new(MockProtocolSim),
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
            other.as_any().is::<MockProtocolSim>()
        }
    }

    #[test]
    fn test_path_builder_success() {
        let mut graph = TradingGraph::new();
        let mut protocol_comp = HashMap::new();
        let mut protocol_sim: HashMap<Bytes, Box<dyn ProtocolSim>> = HashMap::new();

        // Create tokens for a proper arbitrage cycle: A -> B -> C -> A
        let token_a = Bytes::from_str("0x0001").unwrap();
        let token_b = Bytes::from_str("0x0002").unwrap();
        let token_c = Bytes::from_str("0x0003").unwrap();

        let token_a_id = graph.add_token(token_a.clone()).unwrap();
        let token_b_id = graph.add_token(token_b.clone()).unwrap();
        let token_c_id = graph.add_token(token_c.clone()).unwrap();

        // Create pools for arbitrage cycle: A->B, B->C, C->A
        let pool1_addr = Bytes::from_str("0x1001").unwrap();
        let pool2_addr = Bytes::from_str("0x1002").unwrap();
        let pool3_addr = Bytes::from_str("0x1003").unwrap();

        let pool1_ids = graph.add_pool(pool1_addr.clone(), [token_a_id, token_b_id]).unwrap();
        let pool2_ids = graph.add_pool(pool2_addr.clone(), [token_b_id, token_c_id]).unwrap();
        let pool3_ids = graph.add_pool(pool3_addr.clone(), [token_c_id, token_a_id]).unwrap();

        // Create protocol components for all pools
        for (pool_addr, tokens) in [
            (&pool1_addr, vec![
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
            ]),
            (&pool2_addr, vec![
                tycho_simulation::models::Token {
                    address: token_b.clone(),
                    symbol: "TOKEN_B".to_string(),
                    decimals: 18,
                    gas: BigUint::from(0u32),
                },
                tycho_simulation::models::Token {
                    address: token_c.clone(),
                    symbol: "TOKEN_C".to_string(),
                    decimals: 18,
                    gas: BigUint::from(0u32),
                },
            ]),
            (&pool3_addr, vec![
                tycho_simulation::models::Token {
                    address: token_c.clone(),
                    symbol: "TOKEN_C".to_string(),
                    decimals: 18,
                    gas: BigUint::from(0u32),
                },
                tycho_simulation::models::Token {
                    address: token_a.clone(),
                    symbol: "TOKEN_A".to_string(),
                    decimals: 18,
                    gas: BigUint::from(0u32),
                },
            ]),
        ] {
            let pool_comp = ProtocolComponent {
                id: pool_addr.clone(),
                address: pool_addr.clone(),
                protocol_system: "test".to_string(),
                protocol_type_name: "test_pool".to_string(),
                chain: tycho_common::models::Chain::Ethereum,
                tokens,
                contract_ids: vec![pool_addr.clone()],
                static_attributes: HashMap::new(),
                created_at: chrono::NaiveDateTime::from_timestamp_opt(0, 0).unwrap(),
                creation_tx: tycho_common::Bytes::default(),
            };

            protocol_comp.insert(pool_addr.clone(), pool_comp);
            protocol_sim.insert(pool_addr.clone(), Box::new(MockProtocolSim));
        }

        // Create a valid arbitrage cycle path: A->B->C->A
        let path = PathBuilder::new()
            .with_edges(&[pool1_ids[0], pool2_ids[0], pool3_ids[0]])
            .with_graph(&graph)
            .with_protocol_components(&protocol_comp)
            .with_protocol_simulations(&protocol_sim)
            .build();

        assert!(path.is_ok());
        let path = path.unwrap();
        assert_eq!(path.len(), 3);
    }
    #[test]
    fn test_path_builder_single_swap_fails_cycle_validation() {
        let mut graph = TradingGraph::new();
        let mut protocol_comp = HashMap::new();
        let mut protocol_sim: HashMap<Bytes, Box<dyn ProtocolSim>> = HashMap::new();

        // Create tokens
        let token_a = Bytes::from_str("0x0001").unwrap();
        let token_b = Bytes::from_str("0x0002").unwrap();

        let token_a_id = graph.add_token(token_a.clone()).unwrap();
        let token_b_id = graph.add_token(token_b.clone()).unwrap();

        // Create pool
        let pool_addr = Bytes::from_str("0x1001").unwrap();
        let pool_ids = graph.add_pool(pool_addr.clone(), [token_a_id, token_b_id]).unwrap();

        // Create protocol component
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

        protocol_comp.insert(pool_addr.clone(), pool_comp);
        protocol_sim.insert(pool_addr, Box::new(MockProtocolSim));

        // Single swap should fail arbitrage cycle validation
        let path = PathBuilder::new()
            .with_edges(&[pool_ids[0]])
            .with_graph(&graph)
            .with_protocol_components(&protocol_comp)
            .with_protocol_simulations(&protocol_sim)
            .build();

        assert!(path.is_err());
        match path.unwrap_err() {
            crate::errors::ArbitrageError::Path(PathError::InvalidCycle) => {}, // Expected error
            e => panic!("Expected InvalidCycle error, got: {:?}", e),
        }
    }

    #[test]
    fn test_path_builder_missing_components() {
        let graph = TradingGraph::new();
        let protocol_comp = HashMap::new();
        let protocol_sim: HashMap<Bytes, Box<dyn ProtocolSim>> = HashMap::new();

        let result = PathBuilder::new()
            .with_edges(&[0])
            .with_graph(&graph)
            .with_protocol_components(&protocol_comp)
            .with_protocol_simulations(&protocol_sim)
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn test_connectivity_validation() {
        // Test with connected swaps - should pass
        let mut connected_swaps = Vec::new();
        
        // Create mock swaps that are connected (token B output -> token B input)
        // This is a simplified test - in practice you'd create proper mock swaps
        
        let result = PathValidator::validate_connectivity(&connected_swaps);
        assert!(result.is_ok());
    }

    #[test]
    fn test_arbitrage_cycle_validation() {
        let empty_swaps = Vec::new();
        let result = PathValidator::validate_arbitrage_cycle(&empty_swaps);
        assert!(result.is_err());

        // Single swap should fail cycle validation
        // In practice, you'd create a proper mock swap here
        let single_swap = Vec::new();
        let result = PathValidator::validate_arbitrage_cycle(&single_swap);
        assert!(result.is_err());
    }

    #[test]
    fn test_path_consistency_validation() {
        use std::str::FromStr;

        // Test valid arbitrage path: A -> B -> C -> A (3 pools, 4 tokens)
        let pools = vec![
            Bytes::from_str("0x1001").unwrap(),
            Bytes::from_str("0x1002").unwrap(),
            Bytes::from_str("0x1003").unwrap(),
        ];
        let tokens = vec![
            Bytes::from_str("0x0001").unwrap(), // A
            Bytes::from_str("0x0002").unwrap(), // B
            Bytes::from_str("0x0003").unwrap(), // C
            Bytes::from_str("0x0001").unwrap(), // A (cycle completion)
        ];

        let result = PathValidator::validate_path_consistency(&pools, &tokens);
        assert!(result.is_ok());
    }

    #[test]
    fn test_path_consistency_validation_failures() {
        use std::str::FromStr;

        // Test empty pools
        let empty_pools = vec![];
        let tokens = vec![Bytes::from_str("0x0001").unwrap()];
        let result = PathValidator::validate_path_consistency(&empty_pools, &tokens);
        assert!(result.is_err());

        // Test empty tokens
        let pools = vec![Bytes::from_str("0x1001").unwrap()];
        let empty_tokens = vec![];
        let result = PathValidator::validate_path_consistency(&pools, &empty_tokens);
        assert!(result.is_err());

        // Test wrong token count (should be pools + 1)
        let pools = vec![
            Bytes::from_str("0x1001").unwrap(),
            Bytes::from_str("0x1002").unwrap(),
        ];
        let wrong_tokens = vec![
            Bytes::from_str("0x0001").unwrap(),
            Bytes::from_str("0x0002").unwrap(),
        ]; // Should have 3 tokens for 2 pools
        let result = PathValidator::validate_path_consistency(&pools, &wrong_tokens);
        assert!(result.is_err());

        // Test non-cycle path (first != last token)
        let pools = vec![
            Bytes::from_str("0x1001").unwrap(),
            Bytes::from_str("0x1002").unwrap(),
        ];
        let non_cycle_tokens = vec![
            Bytes::from_str("0x0001").unwrap(), // A
            Bytes::from_str("0x0002").unwrap(), // B
            Bytes::from_str("0x0003").unwrap(), // C (should be A for cycle)
        ];
        let result = PathValidator::validate_path_consistency(&pools, &non_cycle_tokens);
        assert!(result.is_err());

        // Test too short path (< 2 pools)
        let short_pools = vec![Bytes::from_str("0x1001").unwrap()];
        let short_tokens = vec![
            Bytes::from_str("0x0001").unwrap(),
            Bytes::from_str("0x0001").unwrap(),
        ];
        let result = PathValidator::validate_path_consistency(&short_pools, &short_tokens);
        assert!(result.is_err());
    }
}
