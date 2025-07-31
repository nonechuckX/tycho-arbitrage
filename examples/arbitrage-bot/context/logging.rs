//! Logging module for arbitrage data collection.
//!
//! This module provides comprehensive logging capabilities for arbitrage operations,
//! storing tabular data in CSV files for analysis and monitoring.

use anyhow::Result;
use chrono::{DateTime, Utc};
use csv::Writer;
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tycho_atomic_arbitrage::path::PathExt;
use tycho_common::Bytes;

/// Configuration data for a single arbitrage run.
///
/// This struct captures all the static parameters and settings used
/// for an arbitrage run, providing a complete audit trail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunConfiguration {
    /// Timestamp when the run was started
    pub timestamp: DateTime<Utc>,
    /// Target blockchain
    pub chain: String,
    /// RPC URL for on-chain interaction (masked for security)
    pub rpc_url_masked: String,
    /// Whether Tycho API key was provided (masked for security)
    pub has_tycho_api_key: bool,
    /// List of start token symbols/addresses
    pub start_tokens: Vec<String>,
    /// Resolved start token addresses
    pub start_token_addresses: Vec<String>,
    /// Optimization tolerance percentages for each start token
    pub optimization_tolerances: Vec<f64>,
    /// Whether executor private key was provided (masked for security)
    pub has_executor_private_key: bool,
    /// Minimum TVL for pools to consider
    pub tvl_threshold: f64,
    /// Minimum profit in BPS for optimization
    pub min_profit_bps: u64,
    /// Slippage tolerance in BPS for trades
    pub slippage_bps: u64,
    /// Whether Flashbots identity key was provided (masked for security)
    pub has_flashbots_identity: bool,
    /// Bribe percentage of expected profit
    pub bribe_percentage: u64,
    /// Native token address for this chain
    pub native_token_address: String,
    /// Tycho URL for this chain
    pub tycho_url: String,
}

impl RunConfiguration {
    /// Mask sensitive parts of a URL for logging
    pub fn mask_url(url: &str) -> String {
        if let Ok(parsed_url) = url::Url::parse(url) {
            let host = parsed_url.host_str().unwrap_or("unknown");
            let scheme = parsed_url.scheme();
            format!("{}://{}/**masked**", scheme, host)
        } else {
            "**masked**".to_string()
        }
    }
}

/// Block-level statistics for arbitrage operations.
#[derive(Debug, Clone, Default)]
pub struct BlockSummary {
    pub block_number: u64,
    pub initial_paths: usize,
    pub candidate_paths: usize,
    pub optimised_profitable_paths: usize,
    pub successful_simulations: usize,
    pub profitable_simulations: usize,
}

/// Main logger for arbitrage operations.
///
/// Manages four CSV files:
/// 1. paths.csv - All generated paths with IDs, pools, and tokens
/// 2. filtered_paths.csv - Blockwise data of filtered/optimized paths
/// 3. simulation_results.csv - Simulation results with gas usage
/// 4. block_summary.csv - Block-level statistics and performance metrics
pub struct PathLogger {
    paths_writer: Arc<Mutex<Writer<File>>>,
    filtered_paths_writer: Arc<Mutex<Writer<File>>>,
    simulation_results_writer: Arc<Mutex<Writer<File>>>,
    block_summary_writer: Arc<Mutex<Writer<File>>>,
    path_id_counter: Arc<Mutex<u64>>,
    path_id_map: Arc<Mutex<HashMap<String, u64>>>,
    run_directory: PathBuf,
}

impl PathLogger {
    /// Create a new PathLogger with output files in a timestamped run directory.
    pub fn new<P: AsRef<Path>>(base_output_dir: P) -> Result<Self> {
        let base_output_dir = base_output_dir.as_ref();
        
        // Generate timestamp for this run
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| anyhow::anyhow!("Failed to get system time: {}", e))?
            .as_secs();
        
        // Create run-specific directory
        let run_dir_name = format!("run_{}", timestamp);
        let output_dir = base_output_dir.join(run_dir_name);
        std::fs::create_dir_all(&output_dir)?;

        // Create CSV writers for each file
        let paths_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(output_dir.join("paths.csv"))?;
        let mut paths_writer = Writer::from_writer(paths_file);
        paths_writer.write_record(&["path_id", "pools", "tokens"])?;
        paths_writer.flush()?;

        let filtered_paths_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(output_dir.join("filtered_and_optimised_paths.csv"))?;
        let mut filtered_paths_writer = Writer::from_writer(filtered_paths_file);
        filtered_paths_writer.write_record(&[
            "block_number",
            "start_token",
            "path_id",
            "spot_price_product", 
            "optimal_input_amount",
            "optimal_output_amount"
        ])?;
        filtered_paths_writer.flush()?;

        let simulation_results_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(output_dir.join("simulation_results.csv"))?;
        let mut simulation_results_writer = Writer::from_writer(simulation_results_file);
        simulation_results_writer.write_record(&[
            "block_number",
            "start_token",
            "path_id",
            "simulation_input_amount",
            "simulation_output_amount", 
            "gas_used",
            "gas_cost",
            "gross_profit_in_native"
        ])?;
        simulation_results_writer.flush()?;

        let block_summary_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(output_dir.join("block_summary.csv"))?;
        let mut block_summary_writer = Writer::from_writer(block_summary_file);
        block_summary_writer.write_record(&[
            "block_number",
            "initial_paths",
            "candidate_paths",
            "optimised_profitable_paths",
            "successful_simulations",
            "profitable_simulations"
        ])?;
        block_summary_writer.flush()?;

        tracing::info!(
            output_directory = %output_dir.display(),
            "Logger initialized"
        );

        Ok(Self {
            paths_writer: Arc::new(Mutex::new(paths_writer)),
            filtered_paths_writer: Arc::new(Mutex::new(filtered_paths_writer)),
            simulation_results_writer: Arc::new(Mutex::new(simulation_results_writer)),
            block_summary_writer: Arc::new(Mutex::new(block_summary_writer)),
            path_id_counter: Arc::new(Mutex::new(1)),
            path_id_map: Arc::new(Mutex::new(HashMap::new())),
            run_directory: output_dir,
        })
    }

    /// Log a generated path with its pools and tokens.
    ///
    /// This creates a unique ID for the path based on its pool sequence
    /// and stores the mapping for later reference.
    pub fn log_path(&self, pools: &[Bytes], tokens: &[Bytes]) -> Result<u64> {
        let path_signature = self.create_path_signature(pools);
        
        // Check if we already have an ID for this path
        {
            let path_map = self.path_id_map.lock().unwrap();
            if let Some(&existing_id) = path_map.get(&path_signature) {
                return Ok(existing_id);
            }
        }

        // Generate new ID
        let path_id = {
            let mut counter = self.path_id_counter.lock().unwrap();
            let id = *counter;
            *counter += 1;
            id
        };

        // Store the mapping
        {
            let mut path_map = self.path_id_map.lock().unwrap();
            path_map.insert(path_signature, path_id);
        }

        // Write to CSV
        {
            let mut writer = self.paths_writer.lock().unwrap();
            let pools_str = pools.iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let tokens_str = tokens.iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>()
                .join(",");
            
            writer.write_record(&[
                path_id.to_string(),
                pools_str,
                tokens_str,
            ])?;
            writer.flush()?;
        }

        tracing::debug!(
            path_id = path_id,
            pools_count = pools.len(),
            tokens_count = tokens.len(),
            "Logged new path"
        );

        Ok(path_id)
    }

    /// Log the configuration for this arbitrage run.
    ///
    /// Creates a config.json file with all the static parameters used for the run,
    /// with sensitive data masked for security.
    pub fn log_config(&self, config: RunConfiguration) -> Result<()> {
        let config_file_path = self.run_directory.join("config.json");
        
        let config_json = serde_json::to_string_pretty(&config)
            .map_err(|e| anyhow::anyhow!("Failed to serialize configuration: {}", e))?;
        
        std::fs::write(&config_file_path, config_json)
            .map_err(|e| anyhow::anyhow!("Failed to write config file: {}", e))?;
        
        tracing::info!(
            config_file = %config_file_path.display(),
            chain = %config.chain,
            start_tokens_count = config.start_tokens.len(),
            "Configuration logged to file"
        );
        
        Ok(())
    }

    /// Log filtered path data with optimization results.
    pub fn log_filtered_path(
        &self,
        path_ext: &PathExt,
        spot_price_product: f64,
        block_number: u64,
    ) -> Result<()> {
        // Extract path information
        let pools: Vec<Bytes> = path_ext.iter()
            .map(|swap| swap.pool_comp.id.clone())
            .collect();
        
        let tokens = self.extract_tokens_from_path_ext(path_ext)?;
        let path_id = self.get_or_create_path_id(&pools, &tokens)?;

        let start_token = path_ext.start_token()
            .map_err(|e| anyhow::anyhow!("Failed to get start token: {}", e))?;

        let default_amount = BigUint::from(0u32);
        let optimal_input_amount = path_ext.first()
            .map(|swap| &swap.amount_in)
            .unwrap_or(&default_amount);

        let optimal_output_amount = path_ext.last()
            .map(|swap| &swap.amount_out)
            .unwrap_or(&default_amount);

        // Write to CSV
        {
            let mut writer = self.filtered_paths_writer.lock().unwrap();
            writer.write_record(&[
                block_number.to_string(),
                start_token.to_string(),
                path_id.to_string(),
                spot_price_product.to_string(),
                optimal_input_amount.to_string(),
                optimal_output_amount.to_string(),
            ])?;
            writer.flush()?;
        }

        tracing::debug!(
            path_id = path_id,
            spot_price_product = spot_price_product,
            block_number = block_number,
            "Logged filtered path"
        );

        Ok(())
    }

    /// Log simulation results.
    pub fn log_simulation_result(
        &self,
        path_ext: &PathExt,
        simulation_input_amount: &BigUint,
        simulation_output_amount: &BigUint,
        gas_used: u64,
        gas_cost: &BigUint,
        gross_profit_in_native: &BigUint,
        start_token: &Bytes,
        block_number: u64,
    ) -> Result<()> {
        // Extract path information
        let pools: Vec<Bytes> = path_ext.iter()
            .map(|swap| swap.pool_comp.id.clone())
            .collect();
        
        let tokens = self.extract_tokens_from_path_ext(path_ext)?;
        let path_id = self.get_or_create_path_id(&pools, &tokens)?;

        // Write to CSV
        {
            let mut writer = self.simulation_results_writer.lock().unwrap();
            writer.write_record(&[
                block_number.to_string(),
                start_token.to_string(),
                path_id.to_string(),
                simulation_input_amount.to_string(),
                simulation_output_amount.to_string(),
                gas_used.to_string(),
                gas_cost.to_string(),
                gross_profit_in_native.to_string(),
            ])?;
            writer.flush()?;
        }

        tracing::debug!(
            path_id = path_id,
            block_number = block_number,
            simulation_input_amount = %simulation_input_amount,
            simulation_output_amount = %simulation_output_amount,
            gas_used = gas_used,
            gas_cost = %gas_cost,
            gross_profit_in_native = %gross_profit_in_native,
            "Logged simulation result"
        );

        Ok(())
    }

    /// Log block-level summary statistics.
    pub fn log_block_summary(&self, summary: &BlockSummary) -> Result<()> {
        // Write to CSV
        {
            let mut writer = self.block_summary_writer.lock().unwrap();
            writer.write_record(&[
                summary.block_number.to_string(),
                summary.initial_paths.to_string(),
                summary.candidate_paths.to_string(),
                summary.optimised_profitable_paths.to_string(),
                summary.successful_simulations.to_string(),
                summary.profitable_simulations.to_string(),
            ])?;
            writer.flush()?;
        }

        tracing::info!(
            block_number = summary.block_number,
            initial_paths = summary.initial_paths,
            candidate_paths = summary.candidate_paths,
            optimised_profitable_paths = summary.optimised_profitable_paths,
            successful_simulations = summary.successful_simulations,
            profitable_simulations = summary.profitable_simulations,
            "Logged block summary"
        );

        Ok(())
    }

    /// Create a unique signature for a path based on its pool sequence.
    fn create_path_signature(&self, pools: &[Bytes]) -> String {
        pools.iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join("|")
    }

    /// Get existing path ID or create a new one.
    fn get_or_create_path_id(&self, pools: &[Bytes], tokens: &[Bytes]) -> Result<u64> {
        let path_signature = self.create_path_signature(pools);
        
        // Check if we already have an ID for this path
        {
            let path_map = self.path_id_map.lock().unwrap();
            if let Some(&existing_id) = path_map.get(&path_signature) {
                return Ok(existing_id);
            }
        }

        // Create new path entry
        self.log_path(pools, tokens)
    }

    /// Extract ordered tokens from a PathExt.
    fn extract_tokens_from_path_ext(&self, path_ext: &PathExt) -> Result<Vec<Bytes>> {
        let mut tokens = Vec::new();
        
        if let Some(first_swap) = path_ext.first() {
            // Add the input token of the first swap
            tokens.push(first_swap.token_in().address.clone());
            
            // Add the output token of each swap
            for swap in path_ext.iter() {
                tokens.push(swap.token_out().address.clone());
            }
        }

        Ok(tokens)
    }
}

impl Clone for PathLogger {
    fn clone(&self) -> Self {
        Self {
            paths_writer: Arc::clone(&self.paths_writer),
            filtered_paths_writer: Arc::clone(&self.filtered_paths_writer),
            simulation_results_writer: Arc::clone(&self.simulation_results_writer),
            block_summary_writer: Arc::clone(&self.block_summary_writer),
            path_id_counter: Arc::clone(&self.path_id_counter),
            path_id_map: Arc::clone(&self.path_id_map),
            run_directory: self.run_directory.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_path_logger_creation() {
        let temp_dir = TempDir::new().unwrap();
        let logger = PathLogger::new(temp_dir.path()).unwrap();
        
        // Verify run directory was created and files exist within it
        let run_dir = &logger.run_directory;
        assert!(run_dir.exists());
        assert!(run_dir.join("paths.csv").exists());
        assert!(run_dir.join("filtered_and_optimised_paths.csv").exists());
        assert!(run_dir.join("simulation_results.csv").exists());
        assert!(run_dir.join("block_summary.csv").exists());
        
        // Verify the run directory name follows the expected pattern
        let dir_name = run_dir.file_name().unwrap().to_str().unwrap();
        assert!(dir_name.starts_with("run_"));
    }

    #[test]
    fn test_path_signature_creation() {
        let temp_dir = TempDir::new().unwrap();
        let logger = PathLogger::new(temp_dir.path()).unwrap();
        
        let pools = vec![
            Bytes::from("0x1234".as_bytes()),
            Bytes::from("0x5678".as_bytes()),
        ];
        
        let signature = logger.create_path_signature(&pools);
        assert!(signature.contains("0x1234"));
        assert!(signature.contains("0x5678"));
        assert!(signature.contains("|"));
    }
}
