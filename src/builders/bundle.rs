//! Builder pattern for TxExecutor

use crate::bundle::TxExecutor;
use crate::config::ArbitrageConfig;
use crate::errors::Result;

/// Builder for creating TxExecutor instances with a fluent API
pub struct TxExecutorBuilder {
    config: Option<ArbitrageConfig>,
}

impl TxExecutorBuilder {
    /// Create a new TxExecutorBuilder
    pub fn new() -> Self {
        Self {
            config: None,
        }
    }

    /// Set the arbitrage configuration
    pub fn with_config(mut self, config: ArbitrageConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Build the TxExecutor
    /// 
    /// # Errors
    /// 
    /// Returns an error if no configuration was provided
    pub fn build(self) -> Result<TxExecutor> {
        let config = self.config
            .ok_or_else(|| crate::errors::BundleError::InvalidConfiguration {
                message: "Configuration is required to build TxExecutor".to_string(),
            })?;

        TxExecutor::from_config(config)
    }
}

impl Default for TxExecutorBuilder {
    fn default() -> Self {
        Self::new()
    }
}
