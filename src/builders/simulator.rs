//! Builder pattern for Simulator

use crate::simulation::Simulator;

/// Builder for creating Simulator instances with a fluent API
pub struct SimulatorBuilder {
    config: crate::config::ArbitrageConfig,
}

impl SimulatorBuilder {
    /// Create a SimulatorBuilder from an ArbitrageConfig
    /// 
    /// # Arguments
    /// 
    /// * `config` - The arbitrage configuration to use
    pub fn from_config(config: &crate::config::ArbitrageConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }

    /// Build the Simulator
    /// 
    /// Creates a new Simulator instance using the provided configuration.
    pub fn build(self) -> Simulator {
        Simulator::from_config(&self.config)
    }
}
