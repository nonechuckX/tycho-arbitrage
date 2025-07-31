//! Builder patterns for complex object construction.
//!
//! This module provides fluent builder patterns for creating complex objects throughout
//! the library. Builder patterns offer a more ergonomic and flexible alternative to
//! constructors with many parameters, allowing for step-by-step configuration with
//! validation at each stage.
//!
//! # Available Builders
//!
//! - **`TxExecutorBuilder`**: Constructs transaction executors with custom configuration
//! - **`TradingGraphBuilder`**: Builds trading graphs with incremental validation
//! - **`SimulatorBuilder`**: Creates simulation engines with configurable parameters
//!
//! # Design Principles
//!
//! All builders in this module follow consistent design patterns:
//!
//! - **Fluent Interface**: Method chaining for readable configuration
//! - **Validation**: Input validation at each step with clear error messages
//! - **Immutability**: Builders consume themselves to prevent reuse after building
//! - **Type Safety**: Compile-time guarantees that required fields are set
//! - **Flexibility**: Optional parameters with sensible defaults
//!
//! # Error Handling
//!
//! Builders validate configuration incrementally and provide detailed error messages
//! when invalid combinations or missing required fields are detected. All build
//! methods return `Result<T>` to handle configuration errors gracefully.

pub mod bundle;
pub mod graph;
pub mod simulator;

// Re-export builders for convenience
pub use bundle::TxExecutorBuilder;
pub use graph::TradingGraphBuilder;
pub use simulator::SimulatorBuilder;
