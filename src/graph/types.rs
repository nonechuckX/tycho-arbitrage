//! Core types and data structures for the trading graph.
//!
//! This module contains the fundamental types used throughout the graph system:
//! - Type aliases for identifiers
//! - Token node representation
//! - Liquidity pool representation
//! - Pool information structures

use std::collections::HashSet;
use tycho_common::Bytes;

/// Type alias for token identifiers within the graph
pub type TokenId = usize;

/// Type alias for pool identifiers within the graph
pub type PoolId = usize;

/// Information about a pool insertion operation
#[derive(Debug, Clone)]
pub struct PoolInfo {
    /// The token IDs that this pool connects
    pub token_ids: [TokenId; 2],
    /// The pool IDs for both directions of the trading pair
    pub pool_ids: [PoolId; 2],
}

/// Represents a token/asset node in the trading graph.
///
/// Each token node maintains its address and a set of neighboring tokens
/// that it can be directly traded with through liquidity pools.
#[derive(Debug, Clone)]
pub struct TokenNode {
    /// The on-chain address of this token
    address: Bytes,
    /// Set of token IDs that this token can be directly traded with
    neighbors: HashSet<TokenId>,
}

impl TokenNode {
    /// Create a new token node with the given address
    pub fn new(address: Bytes) -> Self {
        Self {
            address,
            neighbors: HashSet::new(),
        }
    }

    /// Get the address of this token
    pub fn address(&self) -> &Bytes {
        &self.address
    }

    /// Get the neighboring tokens that can be directly traded with this token
    pub fn neighbors(&self) -> &HashSet<TokenId> {
        &self.neighbors
    }

    /// Get the number of direct trading pairs for this token
    pub fn neighbor_count(&self) -> usize {
        self.neighbors.len()
    }

    /// Add a neighbor token ID (internal use)
    pub(crate) fn add_neighbor(&mut self, token_id: TokenId) {
        self.neighbors.insert(token_id);
    }

    /// Remove a neighbor token ID (internal use)
    pub(crate) fn remove_neighbor(&mut self, token_id: TokenId) {
        self.neighbors.remove(&token_id);
    }
}

/// Represents a liquidity pool/trading pair edge in the trading graph.
///
/// Each pool connects exactly two tokens and has a specific direction
/// (token_in -> token_out) for trading operations.
#[derive(Debug, Clone)]
pub struct LiquidityPool {
    /// The on-chain address of this liquidity pool
    address: Bytes,
    /// The two token IDs that this pool connects [token_in, token_out]
    tokens: [TokenId; 2],
}

impl LiquidityPool {
    /// Create a new liquidity pool connecting the specified tokens
    pub fn new(address: Bytes, tokens: [TokenId; 2]) -> Self {
        Self { address, tokens }
    }

    /// Get the address of this liquidity pool
    pub fn address(&self) -> &Bytes {
        &self.address
    }

    /// Get the token IDs that this pool connects
    pub fn tokens(&self) -> [TokenId; 2] {
        self.tokens
    }

    /// Get the input token ID for this directed pool
    pub fn token_in_id(&self) -> TokenId {
        self.tokens[0]
    }

    /// Get the output token ID for this directed pool
    pub fn token_out_id(&self) -> TokenId {
        self.tokens[1]
    }
}

impl PartialEq for LiquidityPool {
    fn eq(&self, other: &Self) -> bool {
        self.address == other.address
    }
}

impl Eq for LiquidityPool {}
