//! Core trading graph implementation.
//!
//! This module contains the main `TradingGraph` struct and all its methods
//! for managing token trading networks and liquidity pools.

use crate::errors::{GraphError, Result};
use super::types::{TokenId, PoolId, PoolInfo, TokenNode, LiquidityPool};
use std::collections::{HashMap, HashSet};
use tycho_common::Bytes;
use tycho_simulation::protocol::models::ProtocolComponent;

/// A specialized graph data structure for modeling token trading networks.
///
/// The `TradingGraph` represents a network where:
/// - Nodes are tokens/assets that can be traded
/// - Edges are liquidity pools that enable trading between token pairs
/// - The graph supports bidirectional trading (each pool creates two directed edges)
#[derive(Debug)]
pub struct TradingGraph {
    /// Vector of all token nodes in the graph
    tokens: Vec<TokenNode>,
    /// Vector of all liquidity pools in the graph
    pools: Vec<LiquidityPool>,
    /// Mapping from token address to token ID for fast lookup
    token_address_to_id: HashMap<Bytes, TokenId>,
    /// Mapping from token pairs to pool IDs for fast pool lookup
    token_pair_to_pools: HashMap<[TokenId; 2], Vec<PoolId>>,
}

impl TradingGraph {
    /// Create a new empty trading graph
    pub fn new() -> Self {
        Self {
            tokens: Vec::new(),
            pools: Vec::new(),
            token_address_to_id: HashMap::new(),
            token_pair_to_pools: HashMap::new(),
        }
    }

    // ================================
    // Construction Methods
    // ================================

    /// Add a token to the trading graph.
    ///
    /// If a token with the same address already exists, returns the existing token ID.
    ///
    /// # Arguments
    ///
    /// * `address` - The on-chain address of the token to add
    ///
    /// # Returns
    ///
    /// The token ID that can be used to reference this token in other operations
    pub fn add_token(&mut self, address: Bytes) -> Result<TokenId> {
        if let Some(&existing_id) = self.token_address_to_id.get(&address) {
            return Ok(existing_id);
        }
        
        let token_id = self.tokens.len();
        self.tokens.push(TokenNode::new(address.clone()));
        self.token_address_to_id.insert(address, token_id);
        Ok(token_id)
    }

    /// Add a liquidity pool connecting two tokens.
    ///
    /// This creates bidirectional trading capability between the two tokens.
    /// The pool will be added in both directions to support trading in either direction.
    ///
    /// # Arguments
    ///
    /// * `address` - The on-chain address of the liquidity pool
    /// * `token_ids` - Array of exactly 2 token IDs that this pool connects
    ///
    /// # Returns
    ///
    /// A `PoolInfo` struct containing the token IDs and both directional pool IDs
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Either token ID doesn't exist in the graph
    /// - A pool with the same address already exists between these tokens
    pub fn add_pool(&mut self, address: Bytes, token_ids: [TokenId; 2]) -> Result<[PoolId; 2]> {
        // Validate that both tokens exist
        for &token_id in &token_ids {
            if token_id >= self.tokens.len() {
                return Err(GraphError::NonExistentNode { index: token_id }.into());
            }
        }

        let reversed_tokens = [token_ids[1], token_ids[0]];

        // Add pools in both directions
        let pool_id_1 = self.add_pool_directed(address.clone(), token_ids)?;
        let pool_id_2 = self.add_pool_directed(address, reversed_tokens)?;

        Ok([pool_id_1, pool_id_2])
    }

    /// Remove a token and all its associated pools from the graph.
    ///
    /// This operation will also remove all liquidity pools that involve this token.
    ///
    /// # Arguments
    ///
    /// * `token_id` - The ID of the token to remove
    ///
    /// # Errors
    ///
    /// Returns an error if the token ID is invalid
    pub fn remove_token(&mut self, token_id: TokenId) -> Result<()> {
        if token_id >= self.tokens.len() {
            return Err(GraphError::InvalidNodeIndex { index: token_id }.into());
        }

        // Collect all pools to remove (both directions)
        let mut pools_to_remove = Vec::new();
        for &neighbor_id in self.token_neighbors(token_id)?.iter() {
            if let Ok(pool_ids) = self.pools_between_tokens([token_id, neighbor_id]) {
                for &pool_id in pool_ids.iter() {
                    pools_to_remove.push((self.pools[pool_id].address().clone(), [token_id, neighbor_id]));
                }
            }
        }

        // Remove all associated pools
        for (pool_address, token_pair) in pools_to_remove.iter() {
            let _ = self.remove_pool_by_address_and_tokens(pool_address, token_pair);
        }

        // Handle swap-remove index updates
        let last_token_id = self.tokens.len() - 1;
        if token_id != last_token_id {
            // Update the address mapping for the token that will be moved
            if let Some(entry) = self.token_address_to_id.get_mut(self.tokens[last_token_id].address()) {
                *entry = token_id;
            }
        }

        // Remove the token
        self.token_address_to_id.remove(self.tokens[token_id].address());
        self.tokens.swap_remove(token_id);

        Ok(())
    }

    /// Remove a liquidity pool by its address.
    ///
    /// This removes all pools with the given address, regardless of token pairs.
    ///
    /// # Arguments
    ///
    /// * `pool_address` - The on-chain address of the pool to remove
    ///
    /// # Errors
    ///
    /// Returns an error if no pool with the given address exists
    pub fn remove_pool_by_address(&mut self, pool_address: &Bytes) -> Result<()> {
        // Find all pools with this address and collect their token pairs
        let mut pools_to_remove = Vec::new();
        for pool in &self.pools {
            if pool.address() == pool_address {
                pools_to_remove.push(pool.tokens());
            }
        }

        if pools_to_remove.is_empty() {
            return Err(GraphError::EdgeNotFound { address: pool_address.clone() }.into());
        }

        // Remove all pools with this address
        for token_pair in pools_to_remove {
            // Try to remove, but ignore errors if already removed
            let _ = self.remove_pool_by_address_and_tokens(pool_address, &token_pair);
        }

        Ok(())
    }

    // ================================
    // Query Methods
    // ================================

    /// Get the total number of tokens in the graph
    pub fn token_count(&self) -> usize {
        self.tokens.len()
    }

    /// Get the total number of unique pools in the graph
    /// 
    /// Note: This returns the number of unique pool addresses, not directional pool entries.
    /// Multiple token pairs can share the same pool address.
    pub fn pool_count(&self) -> usize {
        let mut unique_addresses = std::collections::HashSet::new();
        for pool in &self.pools {
            unique_addresses.insert(pool.address());
        }
        unique_addresses.len()
    }

    /// Find the token ID for a given token address
    ///
    /// # Arguments
    ///
    /// * `address` - The token address to look up
    ///
    /// # Returns
    ///
    /// The token ID if found
    ///
    /// # Errors
    ///
    /// Returns an error if no token with the given address exists
    pub fn find_token_id(&self, address: &Bytes) -> Result<TokenId> {
        self.token_address_to_id
            .get(address)
            .copied()
            .ok_or_else(|| GraphError::NodeNotFound { address: address.clone() }.into())
    }

    /// Get a token node by its ID
    ///
    /// # Arguments
    ///
    /// * `token_id` - The ID of the token to retrieve
    ///
    /// # Returns
    ///
    /// A reference to the token node
    ///
    /// # Errors
    ///
    /// Returns an error if the token ID is invalid
    pub fn get_token(&self, token_id: TokenId) -> Result<&TokenNode> {
        self.tokens
            .get(token_id)
            .ok_or_else(|| GraphError::InvalidNodeIndex { index: token_id }.into())
    }

    /// Get a liquidity pool by its ID
    ///
    /// # Arguments
    ///
    /// * `pool_id` - The ID of the pool to retrieve
    ///
    /// # Returns
    ///
    /// A reference to the liquidity pool
    ///
    /// # Errors
    ///
    /// Returns an error if the pool ID is invalid
    pub fn get_pool(&self, pool_id: PoolId) -> Result<&LiquidityPool> {
        self.pools
            .get(pool_id)
            .ok_or_else(|| GraphError::InvalidEdgeIndex { index: pool_id }.into())
    }

    /// Get all pools in the graph
    ///
    /// # Returns
    ///
    /// A slice containing all liquidity pools
    pub fn all_pools(&self) -> &[LiquidityPool] {
        &self.pools
    }

    // ================================
    // Navigation Methods
    // ================================

    /// Get the neighboring tokens for a given token
    ///
    /// Returns the set of token IDs that can be directly traded with the specified token.
    ///
    /// # Arguments
    ///
    /// * `token_id` - The ID of the token whose neighbors to retrieve
    ///
    /// # Returns
    ///
    /// A reference to the set of neighboring token IDs
    ///
    /// # Errors
    ///
    /// Returns an error if the token ID is invalid
    pub fn token_neighbors(&self, token_id: TokenId) -> Result<&HashSet<TokenId>> {
        if token_id >= self.tokens.len() {
            return Err(GraphError::InvalidNodeIndex { index: token_id }.into());
        }

        Ok(self.tokens[token_id].neighbors())
    }

    /// Get all pools that connect two specific tokens
    ///
    /// # Arguments
    ///
    /// * `token_pair` - Array of exactly 2 token IDs
    ///
    /// # Returns
    ///
    /// A reference to the vector of pool IDs connecting these tokens
    ///
    /// # Errors
    ///
    /// Returns an error if no pools exist between the specified tokens
    pub fn pools_between_tokens(&self, token_pair: [TokenId; 2]) -> Result<&Vec<PoolId>> {
        self.token_pair_to_pools
            .get(&token_pair)
            .ok_or_else(|| GraphError::PathNotFound.into())
    }

    // ================================
    // Integration Methods
    // ================================

    /// Add a protocol component as a liquidity pool to the graph
    ///
    /// This is a convenience method that extracts token information from a
    /// `ProtocolComponent` and adds the corresponding pool to the graph.
    /// For pools with 3 or 4 tokens, all possible 2-token pairs are created.
    ///
    /// # Arguments
    ///
    /// * `pool_id` - The address/identifier of the pool
    /// * `pool_component` - The protocol component containing token information
    ///
    /// # Returns
    ///
    /// A `Vec<PoolInfo>` with details about the added pools
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The protocol component doesn't have 2-4 tokens
    /// - Pool addition fails for any reason
    pub fn add_protocol_component(&mut self, pool_id: Bytes, pool_component: ProtocolComponent) -> Result<Vec<PoolInfo>> {
        tracing::debug!(
            pool_address = %pool_id,
            protocol_system = %pool_component.protocol_system,
            protocol_type = %pool_component.protocol_type_name,
            "Adding protocol component to graph"
        );

        // Extract and validate token information
        let token_addresses: Vec<Bytes> = pool_component
            .tokens
            .iter()
            .map(|token| token.address.clone())
            .collect();

        if token_addresses.len() < 2 || token_addresses.len() > 4 {
            tracing::error!(
                pool_address = %pool_id,
                token_count = token_addresses.len(),
                "Invalid token count for pool - expected 2-4 tokens"
            );
            return Err(GraphError::InvalidTokenCount { count: token_addresses.len() }.into());
        }

        // Generate all possible token pairs
        let token_pairs = Self::generate_token_pairs(&token_addresses);
        let mut pool_infos = Vec::new();

        // Add each token pair as a separate pool
        for pair in token_pairs {
            // Add tokens to the graph (or get existing IDs)
            let token_id_0 = self.add_token(pair[0].clone())?;
            let token_id_1 = self.add_token(pair[1].clone())?;
            let token_ids = [token_id_0, token_id_1];

            // Add the pool
            let pool_ids = self.add_pool(pool_id.clone(), token_ids)?;

            pool_infos.push(PoolInfo {
                token_ids,
                pool_ids,
            });
        }

        tracing::info!(
            pool_address = %pool_id,
            token_count = token_addresses.len(),
            pairs_created = pool_infos.len(),
            total_tokens = self.token_count(),
            total_pools = self.pool_count(),
            "Protocol component added successfully to graph"
        );

        Ok(pool_infos)
    }

    /// Remove a protocol component pool from the graph
    ///
    /// # Arguments
    ///
    /// * `pool_id` - The address/identifier of the pool to remove
    ///
    /// # Errors
    ///
    /// Returns an error if the pool doesn't exist
    pub fn remove_protocol_component(&mut self, pool_id: &Bytes) -> Result<()> {
        self.remove_pool_by_address(pool_id)
    }

    // ================================
    // Private Helper Methods
    // ================================

    /// Generate all possible 2-token pairs from a list of token addresses
    fn generate_token_pairs(token_addresses: &[Bytes]) -> Vec<[Bytes; 2]> {
        let mut pairs = Vec::new();
        for i in 0..token_addresses.len() {
            for j in i + 1..token_addresses.len() {
                pairs.push([token_addresses[i].clone(), token_addresses[j].clone()]);
            }
        }
        pairs
    }

    /// Add a directed pool (internal helper method)
    fn add_pool_directed(&mut self, address: Bytes, token_ids: [TokenId; 2]) -> Result<PoolId> {
        let pool_id = self.pools.len();

        // Check for duplicate pools
        if let Some(existing_pools) = self.token_pair_to_pools.get(&token_ids) {
            if existing_pools.iter().any(|&id| self.pools[id].address() == &address) {
                return Err(GraphError::DuplicateEdge { address }.into());
            }
        }

        // Add or update the pool mapping
        match self.token_pair_to_pools.get_mut(&token_ids) {
            Some(pool_list) => {
                pool_list.push(pool_id);
            }
            None => {
                self.token_pair_to_pools.insert(token_ids, vec![pool_id]);
                // Update neighbor relationships
                self.tokens[token_ids[0]].add_neighbor(token_ids[1]);
                self.tokens[token_ids[1]].add_neighbor(token_ids[0]);
            }
        }

        // Add the pool
        self.pools.push(LiquidityPool::new(address, token_ids));

        Ok(pool_id)
    }

    /// Remove a directed pool by address and token pair (internal helper method)
    fn remove_pool_by_address_and_tokens(&mut self, address: &Bytes, token_pair: &[TokenId; 2]) -> Result<()> {
        // Find the pool ID to remove
        let pool_id_to_remove = self
            .token_pair_to_pools
            .get(token_pair)
            .and_then(|pool_ids| {
                pool_ids.iter()
                    .find(|&&id| self.pools[id].address() == address)
                    .copied()
            })
            .ok_or_else(|| GraphError::EdgeNotFound { address: address.clone() })?;

        // Handle swap-remove index updates
        let last_pool_id = self.pools.len() - 1;
        if pool_id_to_remove != last_pool_id {
            let last_pool_tokens = self.pools[last_pool_id].tokens();
            
            // Update the mapping for the pool that will be moved
            if let Some(pool_list) = self.token_pair_to_pools.get_mut(&last_pool_tokens) {
                if let Some(index) = pool_list.iter().position(|&id| id == last_pool_id) {
                    pool_list[index] = pool_id_to_remove;
                }
            }
        }

        // Remove from the token pair mapping
        if let Some(pool_list) = self.token_pair_to_pools.get_mut(token_pair) {
            pool_list.retain(|&id| id != pool_id_to_remove);
            
            // If no more pools between these tokens, remove neighbor relationship
            if pool_list.is_empty() {
                self.token_pair_to_pools.remove(token_pair);
                self.tokens[token_pair[0]].remove_neighbor(token_pair[1]);
                self.tokens[token_pair[1]].remove_neighbor(token_pair[0]);
            }
        }

        // Remove the pool
        self.pools.swap_remove(pool_id_to_remove);

        Ok(())
    }
}

impl Default for TradingGraph {
    fn default() -> Self {
        Self::new()
    }
}
