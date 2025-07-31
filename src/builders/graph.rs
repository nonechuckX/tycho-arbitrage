//! Builder pattern for TradingGraph

use crate::graph::TradingGraph;
use crate::errors::Result;
use tycho_common::Bytes;

/// Builder for creating TradingGraph instances with a fluent API
pub struct TradingGraphBuilder {
    tokens: Vec<Bytes>,
    pools: Vec<(Bytes, [usize; 2])>,
}

impl TradingGraphBuilder {
    /// Create a new TradingGraphBuilder
    pub fn new() -> Self {
        Self {
            tokens: Vec::new(),
            pools: Vec::new(),
        }
    }

    /// Add a token to the graph
    /// 
    /// # Arguments
    /// 
    /// * `address` - The address of the token to add
    pub fn add_token(mut self, address: Bytes) -> Self {
        self.tokens.push(address);
        self
    }

    /// Add multiple tokens to the graph
    /// 
    /// # Arguments
    /// 
    /// * `addresses` - Iterator of token addresses to add
    pub fn add_tokens<I>(mut self, addresses: I) -> Self 
    where
        I: IntoIterator<Item = Bytes>,
    {
        self.tokens.extend(addresses);
        self
    }

    /// Add a liquidity pool to the graph
    /// 
    /// # Arguments
    /// 
    /// * `address` - The address of the pool
    /// * `token_indices` - The indices of the tokens this pool connects
    pub fn add_pool(mut self, address: Bytes, token_indices: [usize; 2]) -> Self {
        self.pools.push((address, token_indices));
        self
    }

    /// Add multiple pools to the graph
    /// 
    /// # Arguments
    /// 
    /// * `pools` - Iterator of (address, token_indices) pairs
    pub fn add_pools<I>(mut self, pools: I) -> Self 
    where
        I: IntoIterator<Item = (Bytes, [usize; 2])>,
    {
        self.pools.extend(pools);
        self
    }

    /// Build the TradingGraph
    /// 
    /// # Errors
    /// 
    /// Returns an error if any pool references non-existent tokens
    pub fn build(self) -> Result<TradingGraph> {
        let mut graph = TradingGraph::new();

        // Add all tokens first
        for address in self.tokens {
            graph.add_token(address)?;
        }

        // Then add all pools
        for (address, token_indices) in self.pools {
            // Validate that token indices exist
            if token_indices[0] >= graph.token_count() || token_indices[1] >= graph.token_count() {
                return Err(crate::errors::GraphError::NonExistentNode { 
                    index: if token_indices[0] >= graph.token_count() { token_indices[0] } else { token_indices[1] }
                }.into());
            }
            graph.add_pool(address, token_indices)?;
        }

        Ok(graph)
    }
}

impl Default for TradingGraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}
