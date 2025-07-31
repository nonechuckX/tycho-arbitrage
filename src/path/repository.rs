//! Path repository for managing collections of trading paths.
//! 
//! This module provides functionality for discovering, storing, and retrieving
//! trading paths from a graph structure. It handles path generation, indexing,
//! and efficient lookup operations for arbitrage path discovery.

use crate::errors::{PathError, Result};
use crate::graph::TradingGraph;
use crate::path::Path;
use std::collections::HashMap;
use tycho_common::Bytes;
use tycho_simulation::{
    protocol::{models::ProtocolComponent, state::ProtocolSim},
};

/// Repository for managing collections of trading paths.
///
/// The `PathRepository` maintains indexed collections of trading paths discovered
/// from a trading graph. It provides efficient lookup and retrieval operations
/// for paths involving specific tokens or pools.
#[derive(Debug, Clone)]
pub struct PathRepository {
    /// Source tokens that serve as starting points for path discovery
    source_tokens: Vec<Bytes>,
    /// Maximum allowed path length (number of swaps)
    maximum_path_length: usize,
    /// Token-based paths (sequences of token indices)
    pub token_paths: Vec<Vec<usize>>,
    /// Pool-based paths (sequences of pool indices)
    pub pool_paths: Vec<Vec<usize>>,
    /// Index mapping tokens to their associated path indices
    token_to_path_indices: HashMap<Bytes, Vec<usize>>,
    /// Index mapping pools to their associated path indices
    pool_to_path_indices: HashMap<Bytes, Vec<usize>>,
}

impl PathRepository {
    /// Create a new path repository.
    ///
    /// # Arguments
    ///
    /// * `source_tokens` - Token addresses that serve as starting points for path discovery
    /// * `maximum_path_length` - Maximum number of swaps allowed in a path
    pub fn new(source_tokens: Vec<Bytes>, maximum_path_length: usize) -> Self {
        tracing::debug!(
            source_token_count = source_tokens.len(),
            maximum_path_length = maximum_path_length,
            "Creating new path repository"
        );

        Self {
            source_tokens,
            maximum_path_length,
            token_paths: Vec::new(),
            pool_paths: Vec::new(),
            token_to_path_indices: HashMap::new(),
            pool_to_path_indices: HashMap::new(),
        }
    }

    /// Get path indices for a specific pool.
    ///
    /// # Arguments
    ///
    /// * `pool_address` - The address of the pool to find paths for
    ///
    /// # Returns
    ///
    /// A reference to the vector of path indices involving this pool
    pub fn get_path_indices_for_pool(&self, pool_address: &Bytes) -> Result<&Vec<usize>> {
        self.pool_to_path_indices
            .get(pool_address)
            .ok_or_else(|| {
                PathError::PoolNotFoundInRepository { 
                    pool: pool_address.clone() 
                }.into()
            })
    }

    /// Get path indices for multiple pools.
    ///
    /// Returns the union of all path indices that involve any of the specified pools.
    ///
    /// # Arguments
    ///
    /// * `pool_addresses` - The addresses of the pools to find paths for
    ///
    /// # Returns
    ///
    /// A deduplicated, sorted vector of path indices
    pub fn get_path_indices_for_pools(&self, pool_addresses: &[Bytes]) -> Result<Vec<usize>> {
        let mut path_indices = Vec::new();

        for pool_address in pool_addresses.iter() {
            if let Ok(indices) = self.get_path_indices_for_pool(pool_address) {
                path_indices.extend(indices.iter().copied());
            }
        }

        path_indices.sort_unstable();
        path_indices.dedup();

        tracing::debug!(
            pool_count = pool_addresses.len(),
            unique_path_count = path_indices.len(),
            "Found paths for multiple pools"
        );

        Ok(path_indices)
    }

    /// Get a pool path at a specific index.
    ///
    /// # Arguments
    ///
    /// * `path_index` - The index of the path to retrieve
    ///
    /// # Returns
    ///
    /// A reference to the vector of pool indices forming the path
    pub fn get_pool_path_by_index(&self, path_index: usize) -> Result<&Vec<usize>> {
        self.pool_paths
            .get(path_index)
            .ok_or_else(|| {
                PathError::InvalidPathIndex { index: path_index }.into()
            })
    }

    /// Discover new paths in the repository based on graph updates.
    ///
    /// This method discovers new trading paths when the graph is updated with new
    /// tokens or pools. It uses incremental discovery to avoid recomputing existing paths.
    ///
    /// # Arguments
    ///
    /// * `graph` - The trading graph to discover paths from
    /// * `new_token_offset` - Starting index of newly added tokens
    /// * `_new_token_count` - Number of newly added tokens (unused but kept for API compatibility)
    /// * `new_pool_offset` - Starting index of newly added pools
    /// * `new_pool_count` - Number of newly added pools
    pub fn discover_paths(
        &mut self,
        graph: &TradingGraph,
        new_token_offset: usize,
        _new_token_count: usize,
        new_pool_offset: usize,
        new_pool_count: usize,
    ) {
        let source_indices = self.resolve_source_token_indices(graph);

        tracing::info!(
            source_token_count = self.source_tokens.len(),
            resolved_source_count = source_indices.len(),
            max_path_length = self.maximum_path_length,
            new_pool_offset = new_pool_offset,
            new_pool_count = new_pool_count,
            "Starting path discovery"
        );

        // Discover token-based paths
        self.discover_token_paths(graph, &source_indices, new_token_offset);

        // Discover pool-based paths from token paths
        self.discover_pool_paths_from_updates(graph, new_pool_offset, new_pool_count);

        tracing::info!(
            total_token_paths = self.token_paths.len(),
            total_pool_paths = self.pool_paths.len(),
            "Path discovery completed"
        );
    }

    /// Resolve source token addresses to their corresponding graph indices.
    fn resolve_source_token_indices(&self, graph: &TradingGraph) -> Vec<usize> {
        let source_indices: Vec<usize> = self
            .source_tokens
            .iter()
            .filter_map(|token_address| {
                match graph.find_token_id(token_address) {
                    Ok(index) => Some(index),
                    Err(_) => {
                        tracing::debug!(
                            token_address = %token_address,
                            "Source token not found in graph"
                        );
                        None
                    }
                }
            })
            .collect();

        tracing::debug!(
            requested_sources = self.source_tokens.len(),
            resolved_sources = source_indices.len(),
            "Resolved source token indices"
        );

        source_indices
    }

    /// Discover token-based paths starting from source tokens.
    fn discover_token_paths(
        &mut self,
        graph: &TradingGraph,
        source_indices: &[usize],
        new_token_offset: usize,
    ) {
        for path_length in 2..=self.maximum_path_length {
            for &source_index in source_indices.iter() {
                self.discover_token_paths_recursive(
                    graph,
                    source_indices,
                    new_token_offset,
                    path_length,
                    vec![source_index],
                );
            }
        }
    }

    /// Recursively discover token paths using depth-first search.
    fn discover_token_paths_recursive(
        &mut self,
        graph: &TradingGraph,
        source_indices: &[usize],
        new_token_offset: usize,
        target_length: usize,
        current_path: Vec<usize>,
    ) {
        let current_token_index = match current_path.last() {
            Some(&index) => index,
            None => {
                tracing::warn!("Empty path in token path discovery");
                return;
            }
        };
        
        let neighbor_indices = match graph.token_neighbors(current_token_index) {
            Ok(indices) => indices,
            Err(e) => {
                tracing::debug!(
                    token_index = current_token_index,
                    error = %e,
                    "Failed to get token neighbors"
                );
                return;
            }
        };

        if target_length == current_path.len() {
            // Check if path forms a cycle back to any source token
            if neighbor_indices.iter().any(|&idx| source_indices.contains(&idx)) {
                self.store_discovered_token_path(graph, current_path);
            }
        } else {
            // Continue exploring neighbors
            for &neighbor_index in neighbor_indices.iter() {
                if self.should_explore_token_neighbor(
                    neighbor_index,
                    new_token_offset,
                    source_indices,
                    &current_path,
                ) {
                    let mut extended_path = current_path.clone();
                    extended_path.push(neighbor_index);

                    self.discover_token_paths_recursive(
                        graph,
                        source_indices,
                        new_token_offset,
                        target_length,
                        extended_path,
                    );
                }
            }
        }
    }

    /// Check if a token neighbor should be explored during path discovery.
    fn should_explore_token_neighbor(
        &self,
        neighbor_index: usize,
        new_token_offset: usize,
        source_indices: &[usize],
        current_path: &[usize],
    ) -> bool {
        // Only explore new tokens and avoid revisiting source tokens in the middle of paths
        neighbor_index >= new_token_offset 
            && !source_indices.contains(&neighbor_index)
            && !current_path.contains(&neighbor_index) // Avoid cycles within the path
    }

    /// Store a discovered token path and update indices.
    fn store_discovered_token_path(&mut self, graph: &TradingGraph, token_path: Vec<usize>) {
        let path_index = self.token_paths.len();

        // Update token-to-path index mapping
        for &token_index in token_path.iter() {
            if let Ok(token) = graph.get_token(token_index) {
                self.token_to_path_indices
                    .entry(token.address().clone())
                    .or_insert_with(Vec::new)
                    .push(path_index);
            }
        }

        self.token_paths.push(token_path);

        tracing::trace!(
            path_index = path_index,
            path_length = self.token_paths[path_index].len(),
            "Stored new token path"
        );
    }

    /// Discover pool-based paths from graph updates.
    fn discover_pool_paths_from_updates(
        &mut self,
        graph: &TradingGraph,
        new_pool_offset: usize,
        new_pool_count: usize,
    ) {
        // Find tokens affected by new pools
        let affected_token_indices = self.find_tokens_affected_by_new_pools(
            graph,
            new_pool_offset,
            new_pool_count,
        );

        // Find relevant token paths that involve affected tokens
        let relevant_token_path_indices = self.find_relevant_token_paths(
            graph,
            &affected_token_indices,
        );

        tracing::debug!(
            affected_tokens = affected_token_indices.len(),
            relevant_token_paths = relevant_token_path_indices.len(),
            "Found paths to update with new pools"
        );

        // Generate pool paths from relevant token paths
        for &token_path_index in relevant_token_path_indices.iter() {
            let token_path = self.token_paths[token_path_index].clone();
            self.discover_pool_paths_recursive(graph, new_pool_offset, &token_path, Vec::new());
        }
    }

    /// Find token indices that are affected by newly added pools.
    fn find_tokens_affected_by_new_pools(
        &self,
        graph: &TradingGraph,
        new_pool_offset: usize,
        new_pool_count: usize,
    ) -> Vec<usize> {
        let mut affected_tokens: Vec<usize> = (new_pool_offset..(new_pool_offset + new_pool_count))
            .filter_map(|pool_index| graph.get_pool(pool_index).ok())
            .flat_map(|pool| pool.tokens())
            .collect();

        affected_tokens.sort_unstable();
        affected_tokens.dedup();

        tracing::debug!(
            new_pool_count = new_pool_count,
            affected_token_count = affected_tokens.len(),
            "Found tokens affected by new pools"
        );

        affected_tokens
    }

    /// Find token path indices that involve any of the specified tokens.
    fn find_relevant_token_paths(
        &self,
        graph: &TradingGraph,
        affected_token_indices: &[usize],
    ) -> Vec<usize> {
        let mut relevant_path_indices: Vec<usize> = affected_token_indices
            .iter()
            .filter_map(|&token_index| graph.get_token(token_index).ok())
            .flat_map(|token| {
                self.token_to_path_indices
                    .get(token.address())
                    .map(|indices| indices.iter().copied())
                    .into_iter()
                    .flatten()
            })
            .collect();

        relevant_path_indices.sort_unstable();
        relevant_path_indices.dedup();

        relevant_path_indices
    }

    /// Recursively discover pool paths from a token path.
    fn discover_pool_paths_recursive(
        &mut self,
        graph: &TradingGraph,
        new_pool_offset: usize,
        token_path: &[usize],
        current_pool_path: Vec<usize>,
    ) {
        let current_position = current_pool_path.len();

        if current_position == token_path.len() {
            // Complete pool path found
            self.store_discovered_pool_path(graph, current_pool_path);
        } else {
            // Find pools connecting current and next tokens
            let current_token = token_path[current_position];
            let next_token = token_path[(current_position + 1) % token_path.len()];
            let token_pair = [current_token, next_token];

            if let Ok(connecting_pools) = graph.pools_between_tokens(token_pair) {
                let should_include_new_pools = self.should_include_new_pools(
                    connecting_pools,
                    new_pool_offset,
                );

                for &pool_index in connecting_pools.iter() {
                    if self.should_use_pool_in_path(
                        graph,
                        pool_index,
                        new_pool_offset,
                        should_include_new_pools,
                        &current_pool_path,
                    ) {
                        let mut extended_pool_path = current_pool_path.clone();
                        extended_pool_path.push(pool_index);

                        self.discover_pool_paths_recursive(
                            graph,
                            new_pool_offset,
                            token_path,
                            extended_pool_path,
                        );
                    }
                }
            }
        }
    }

    /// Check if new pools should be included in path discovery.
    fn should_include_new_pools(&self, connecting_pools: &[usize], new_pool_offset: usize) -> bool {
        connecting_pools
            .iter()
            .any(|&pool_index| pool_index >= new_pool_offset)
    }

    /// Check if a specific pool should be used in the current path.
    fn should_use_pool_in_path(
        &self,
        graph: &TradingGraph,
        pool_index: usize,
        new_pool_offset: usize,
        should_include_new_pools: bool,
        current_pool_path: &[usize],
    ) -> bool {
        // Only include if we're looking for new pools and this is a new pool,
        // or if we're not specifically looking for new pools
        let pool_is_new = pool_index >= new_pool_offset;
        let should_include = !should_include_new_pools || pool_is_new;

        if !should_include {
            return false;
        }

        // Check if pool is already used in the current path (avoid duplicates)
        let pool_already_used = current_pool_path.iter().any(|&existing_pool_index| {
            match (graph.get_pool(existing_pool_index), graph.get_pool(pool_index)) {
                (Ok(existing_pool), Ok(current_pool)) => {
                    existing_pool.address() == current_pool.address()
                }
                _ => false,
            }
        });

        !pool_already_used
    }

    /// Store a discovered pool path and update indices.
    fn store_discovered_pool_path(&mut self, graph: &TradingGraph, pool_path: Vec<usize>) {
        let path_index = self.pool_paths.len();

        // Update pool-to-path index mapping
        for &pool_index in pool_path.iter() {
            if let Ok(pool) = graph.get_pool(pool_index) {
                self.pool_to_path_indices
                    .entry(pool.address().clone())
                    .or_insert_with(Vec::new)
                    .push(path_index);
            }
        }

        self.pool_paths.push(pool_path);

        tracing::trace!(
            path_index = path_index,
            path_length = self.pool_paths[path_index].len(),
            "Stored new pool path"
        );
    }

    /// Convert path indices to actual Path objects.
    ///
    /// This method builds `Path` objects from stored path indices, using the provided
    /// protocol components and simulations. Paths that cannot be built due to missing
    /// protocol data are skipped with appropriate logging.
    ///
    /// # Arguments
    ///
    /// * `path_indices` - Vector of path indices to convert
    /// * `graph` - The trading graph containing pool and token information
    /// * `protocol_simulations` - Map of pool addresses to protocol simulations
    /// * `protocol_components` - Map of pool addresses to protocol components
    ///
    /// # Returns
    ///
    /// A vector of successfully built `Path` objects
    pub fn build_paths_from_indices(
        &self,
        path_indices: Vec<usize>,
        graph: &TradingGraph,
        protocol_simulations: &HashMap<Bytes, Box<dyn ProtocolSim>>,
        protocol_components: &HashMap<Bytes, ProtocolComponent>,
    ) -> Result<Vec<Path>> {
        let mut successfully_built_paths = Vec::new();
        let mut skipped_count = 0;

        tracing::debug!(
            total_indices = path_indices.len(),
            "Building paths from indices"
        );

        for &path_index in path_indices.iter() {
            let pool_indices = self.get_pool_path_by_index(path_index)?;
            
            match self.build_single_path(pool_indices, graph, protocol_components, protocol_simulations) {
                Ok(path) => {
                    successfully_built_paths.push(path);
                }
                Err(e) => {
                    skipped_count += 1;
                    tracing::debug!(
                        path_index = path_index,
                        error = %e,
                        "Skipped path due to build failure"
                    );
                }
            }
        }

        self.log_path_building_results(path_indices.len(), successfully_built_paths.len(), skipped_count);

        Ok(successfully_built_paths)
    }

    /// Build a single path from pool indices.
    fn build_single_path(
        &self,
        pool_indices: &[usize],
        graph: &TradingGraph,
        protocol_components: &HashMap<Bytes, ProtocolComponent>,
        protocol_simulations: &HashMap<Bytes, Box<dyn ProtocolSim>>,
    ) -> Result<Path> {
        use crate::path::creation::PathBuilder;

        PathBuilder::new()
            .with_edges(pool_indices)
            .with_graph(graph)
            .with_protocol_components(protocol_components)
            .with_protocol_simulations(protocol_simulations)
            .build()
    }

    /// Log the results of path building operations.
    fn log_path_building_results(&self, total_indices: usize, successful_paths: usize, skipped_count: usize) {
        if skipped_count > 0 {
            let success_rate = if total_indices > 0 {
                format!("{:.1}%", (successful_paths as f64 / total_indices as f64) * 100.0)
            } else {
                "N/A".to_string()
            };

            tracing::info!(
                total_path_indices = total_indices,
                successful_paths = successful_paths,
                skipped_paths = skipped_count,
                success_rate = success_rate,
                "Path building completed with some paths skipped"
            );
        } else {
            tracing::debug!(
                total_paths = successful_paths,
                "All paths built successfully"
            );
        }
    }

    /// Get paths for specific pools (convenience method).
    ///
    /// This is a high-level method that combines path index lookup and path building.
    ///
    /// # Arguments
    ///
    /// * `pool_addresses` - The addresses of pools to find paths for
    /// * `graph` - The trading graph
    /// * `protocol_components` - Protocol components map
    /// * `protocol_simulations` - Protocol simulations map
    ///
    /// # Returns
    ///
    /// A vector of paths involving the specified pools
    pub fn get_paths_for_pools(
        &self,
        pool_addresses: &[Bytes],
        graph: &TradingGraph,
        protocol_components: &HashMap<Bytes, ProtocolComponent>,
        protocol_simulations: &HashMap<Bytes, Box<dyn ProtocolSim>>,
    ) -> Result<Vec<Path>> {
        let path_indices = self.get_path_indices_for_pools(pool_addresses)?;
        self.build_paths_from_indices(path_indices, graph, protocol_simulations, protocol_components)
    }

    /// Get statistics about the repository.
    pub fn statistics(&self) -> RepositoryStatistics {
        RepositoryStatistics {
            source_token_count: self.source_tokens.len(),
            maximum_path_length: self.maximum_path_length,
            token_path_count: self.token_paths.len(),
            pool_path_count: self.pool_paths.len(),
            indexed_token_count: self.token_to_path_indices.len(),
            indexed_pool_count: self.pool_to_path_indices.len(),
        }
    }
}

/// Statistics about a path repository.
#[derive(Debug, Clone)]
pub struct RepositoryStatistics {
    /// Number of source tokens
    pub source_token_count: usize,
    /// Maximum allowed path length
    pub maximum_path_length: usize,
    /// Number of token-based paths
    pub token_path_count: usize,
    /// Number of pool-based paths
    pub pool_path_count: usize,
    /// Number of tokens with indexed paths
    pub indexed_token_count: usize,
    /// Number of pools with indexed paths
    pub indexed_pool_count: usize,
}

impl std::fmt::Display for RepositoryStatistics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "RepositoryStatistics {{ sources: {}, max_length: {}, token_paths: {}, pool_paths: {}, indexed_tokens: {}, indexed_pools: {} }}",
            self.source_token_count,
            self.maximum_path_length,
            self.token_path_count,
            self.pool_path_count,
            self.indexed_token_count,
            self.indexed_pool_count
        )
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::TradingGraph;
    use std::str::FromStr;

    #[test]
    fn test_path_repository_cycle() {
        let mut g = TradingGraph::new();

        let node1 = Bytes::from_str("0x0000").unwrap();
        let node2 = Bytes::from_str("0x0001").unwrap();
        let node3 = Bytes::from_str("0x0002").unwrap();
        let node4 = Bytes::from_str("0x0003").unwrap();

        let _ = g.add_token(node1.clone());
        let _ = g.add_token(node2);
        let _ = g.add_token(node3.clone());
        let _ = g.add_token(node4);

        let edge1 = Bytes::from_str("0x1000").unwrap();
        let edge2 = Bytes::from_str("0x1001").unwrap();
        let edge3 = Bytes::from_str("0x1002").unwrap();
        let edge4 = Bytes::from_str("0x1003").unwrap();

        let _ = g.add_pool(edge1, [0, 1]).is_ok();
        let _ = g.add_pool(edge2, [1, 2]).is_ok();
        let _ = g.add_pool(edge3, [0, 2]).is_ok();
        let _ = g.add_pool(edge4.clone(), [0, 1]).is_ok();

        let source_node = node1.clone();
        let max_len = 3;

        let mut paths_repo = PathRepository::new(vec![source_node], max_len);

        paths_repo.discover_paths(&g, 0_usize, 4_usize, 0_usize, 4_usize);
        assert!(paths_repo.get_path_indices_for_pool(&edge4).is_ok());
    }
}
