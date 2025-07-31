//! Graph operations and pathfinding errors.

use tycho_common::Bytes;

/// Errors that can occur during graph operations
#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("Node not found with address: {address:?}")]
    NodeNotFound { address: Bytes },

    #[error("Edge not found with address: {address:?}")]
    EdgeNotFound { address: Bytes },

    #[error("Invalid node index: {index}")]
    InvalidNodeIndex { index: usize },

    #[error("Invalid edge index: {index}")]
    InvalidEdgeIndex { index: usize },

    #[error("Node with address {address:?} already exists")]
    DuplicateNode { address: Bytes },

    #[error("Edge with address {address:?} already exists between nodes")]
    DuplicateEdge { address: Bytes },

    #[error("Cannot connect non-existent node at index {index}")]
    NonExistentNode { index: usize },

    #[error("Invalid token count: expected 2, got {count}")]
    InvalidTokenCount { count: usize },

    #[error("Cannot remove node {index}: it has {edge_count} connected edges")]
    NodeHasConnectedEdges { index: usize, edge_count: usize },

    #[error("Graph operation failed: {operation}")]
    OperationFailed { operation: String },

    #[error("Path not found between nodes")]
    PathNotFound,

    #[error("Empty graph: no nodes available")]
    EmptyGraph,

    #[error("Invalid edge configuration: nodes [{node1}, {node2}]")]
    InvalidEdgeConfiguration { node1: usize, node2: usize },
}
