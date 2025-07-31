//! Configuration management for the tycho-atomic-arbitrage library.
//! 
//! This module provides secure configuration loading and validation,
//! replacing hard-coded values with environment-based configuration.

use crate::errors::{BundleError, Result};
use alloy::signers::local::PrivateKeySigner;
use serde::{Deserialize, Serialize};
use std::env;
use std::str::FromStr;

/// Configuration for relayer endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayerConfig {
    /// List of relayer URLs to submit bundles to
    pub urls: Vec<String>,
    /// Timeout for relayer requests in milliseconds
    pub timeout_ms: u64,
}

impl Default for RelayerConfig {
    fn default() -> Self {
        Self {
            urls: vec![
                "https://rpc.titanbuilder.xyz".to_string(),
                "https://rpc.beaverbuild.org".to_string(),
                "https://relay.flashbots.net".to_string(),
            ],
            timeout_ms: 5000,
        }
    }
}

/// Security configuration for private keys and identity management
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// Flashbots identity private key (optional)
    pub flashbots_identity: Option<PrivateKeySigner>,
    /// Executor private key for signing transactions
    pub executor_key: PrivateKeySigner,
    /// Whether to validate private keys on creation
    pub validate_keys: bool,
}

/// Main configuration structure for the arbitrage system
#[derive(Debug, Clone)]
pub struct ArbitrageConfig {
    /// Relayer configuration
    pub relayer: RelayerConfig,
    /// Security configuration
    pub security: SecurityConfig,
    /// Chain configuration
    pub chain_id: u64,
    /// Permit2 contract address for the chain
    pub permit2_address: alloy::primitives::Address,
    /// Bribe percentage (0-100)
    pub bribe_percentage: u64,
}

impl ArbitrageConfig {
    /// Create a new configuration from environment variables
    /// 
    /// # Environment Variables
    /// 
    /// ## Required
    /// - `TYCHO_EXECUTOR_PRIVATE_KEY`: Private key for transaction signing (without 0x prefix)
    /// 
    /// ## Optional (CLI-specific with TYCHO_ prefix)
    /// - `TYCHO_CHAIN`: Target blockchain (default: ethereum)
    /// - `TYCHO_RPC_URL`: RPC URL for on-chain interaction
    /// - `TYCHO_API_KEY`: Tycho API key
    /// - `TYCHO_TVL_THRESHOLD`: Minimum TVL for pools to consider (default: 70.0)
    /// - `TYCHO_MIN_PROFIT_BPS`: Minimum profit in BPS (default: 100)
    /// - `TYCHO_SLIPPAGE_BPS`: Slippage tolerance in BPS (default: 50)
    /// - `TYCHO_FLASHBOTS_IDENTITY_KEY`: Private key for Flashbots authentication
    /// - `TYCHO_BRIBE_PERCENTAGE`: Bribe percentage (default: 99)
    /// 
    /// # Errors
    /// 
    /// Returns an error if:
    /// - Required environment variables are missing
    /// - Private keys are invalid
    /// - Configuration values are out of valid ranges
    pub fn from_env(chain: &str) -> Result<Self> {
        tracing::info!(
            chain = chain,
            "Loading arbitrage configuration from environment"
        );

        // Load executor private key (required)
        let executor_key_str = env::var("TYCHO_EXECUTOR_PRIVATE_KEY")
            .map_err(|_| {
                tracing::error!("TYCHO_EXECUTOR_PRIVATE_KEY environment variable is required but not found");
                BundleError::InvalidConfiguration {
                    message: "TYCHO_EXECUTOR_PRIVATE_KEY environment variable is required".to_string(),
                }
            })?;

        let executor_key = Self::parse_and_validate_private_key(&executor_key_str, "TYCHO_EXECUTOR_PRIVATE_KEY")?;
        tracing::debug!("Executor private key loaded and validated successfully");

        // Load optional flashbots identity key
        let flashbots_identity = if let Ok(identity_key_str) = env::var("FLASHBOTS_IDENTITY_KEY") {
            tracing::debug!("Loading Flashbots identity key from environment");
            Some(Self::parse_and_validate_private_key(&identity_key_str, "FLASHBOTS_IDENTITY_KEY")?)
        } else {
            tracing::debug!("No Flashbots identity key provided - will generate random identity for testing");
            None
        };

        // Load relayer configuration
        let relayer_urls = if let Ok(urls_str) = env::var("RELAYER_URLS") {
            let urls: Vec<String> = urls_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            tracing::debug!(
                relayer_count = urls.len(),
                relayers = ?urls,
                "Custom relayer URLs loaded from environment"
            );
            urls
        } else {
            let default_urls = RelayerConfig::default().urls;
            tracing::debug!(
                relayer_count = default_urls.len(),
                relayers = ?default_urls,
                "Using default relayer URLs"
            );
            default_urls
        };

        let timeout_ms = env::var("RELAYER_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5000);

        tracing::debug!(
            timeout_ms = timeout_ms,
            "Relayer configuration loaded"
        );

        // Validate relayer URLs
        Self::validate_relayer_urls(&relayer_urls)?;

        let relayer = RelayerConfig {
            urls: relayer_urls,
            timeout_ms,
        };

        // Load other configuration
        let bribe_percentage = env::var("BRIBE_PERCENTAGE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(50);

        if bribe_percentage > 100 {
            tracing::error!(
                bribe_percentage = bribe_percentage,
                "Invalid bribe percentage - must be between 0 and 100"
            );
            return Err(BundleError::InvalidConfiguration {
                message: "BRIBE_PERCENTAGE must be between 0 and 100".to_string(),
            }.into());
        }

        let chain_id = crate::utils::chain_id(chain)?;

        // Load permit2 address (with optional override)
        let permit2_address = if let Ok(custom_address) = env::var("PERMIT2_ADDRESS") {
            tracing::debug!(
                custom_address = custom_address,
                "Using custom Permit2 address from environment"
            );
            Self::parse_and_validate_address(&custom_address, "PERMIT2_ADDRESS")?
        } else {
            let default_address = crate::utils::permit2_address(chain)?;
            tracing::debug!(
                permit2_address = %default_address,
                "Using default Permit2 address for chain"
            );
            default_address
        };

        tracing::debug!(
            bribe_percentage = bribe_percentage,
            chain_id = chain_id,
            permit2_address = %permit2_address,
            "Business logic configuration loaded"
        );

        let security = SecurityConfig {
            flashbots_identity,
            executor_key,
            validate_keys: true,
        };

        let config = Self {
            relayer,
            security,
            chain_id,
            permit2_address,
            bribe_percentage,
        };

        // Validate CLI-specific environment variables
        Self::validate_cli_env_vars()?;

        tracing::info!(
            chain = chain,
            chain_id = chain_id,
            relayer_count = config.relayer.urls.len(),
            bribe_percentage = config.bribe_percentage,
            has_flashbots_identity = config.security.flashbots_identity.is_some(),
            "Arbitrage configuration loaded successfully"
        );

        Ok(config)
    }

    /// Create a configuration for testing purposes with secure defaults
    /// 
    /// # Security Note
    /// 
    /// This method generates random private keys and should only be used for testing.
    /// Never use this in production environments.
    #[cfg(test)]
    pub fn for_testing(chain: &str) -> Result<Self> {
        use alloy::signers::local::PrivateKeySigner;
        
        let executor_key = PrivateKeySigner::random();
        let flashbots_identity = Some(PrivateKeySigner::random());
        let chain_id = crate::utils::chain_id(chain)?;
        let permit2_address = crate::utils::permit2_address(chain)?;

        let security = SecurityConfig {
            flashbots_identity,
            executor_key,
            validate_keys: true,
        };

        Ok(Self {
            relayer: RelayerConfig::default(),
            security,
            chain_id,
            permit2_address,
            bribe_percentage: 50,
        })
    }

    /// Validate CLI-specific environment variables and set defaults if not provided
    /// This ensures all TYCHO_ prefixed environment variables are properly validated
    fn validate_cli_env_vars() -> Result<()> {
        tracing::debug!("Validating CLI-specific environment variables");

        // Validate TYCHO_CHAIN if set
        if let Ok(chain) = env::var("TYCHO_CHAIN") {
            match chain.as_str() {
                "ethereum" | "base" | "unichain" => {
                    tracing::debug!(chain = chain, "Valid TYCHO_CHAIN value");
                }
                _ => {
                    return Err(BundleError::InvalidConfiguration {
                        message: format!("Invalid TYCHO_CHAIN value: {}. Must be one of: ethereum, base, unichain", chain),
                    }.into());
                }
            }
        } else {
            tracing::debug!("TYCHO_CHAIN not set, using default: ethereum");
            env::set_var("TYCHO_CHAIN", "ethereum");
        }

        // Validate TYCHO_TVL_THRESHOLD if set
        if let Ok(tvl_str) = env::var("TYCHO_TVL_THRESHOLD") {
            match tvl_str.parse::<f64>() {
                Ok(tvl) if tvl >= 0.0 => {
                    tracing::debug!(tvl_threshold = tvl, "Valid TYCHO_TVL_THRESHOLD value");
                }
                Ok(tvl) => {
                    return Err(BundleError::InvalidConfiguration {
                        message: format!("TYCHO_TVL_THRESHOLD must be non-negative, got: {}", tvl),
                    }.into());
                }
                Err(_) => {
                    return Err(BundleError::InvalidConfiguration {
                        message: format!("Invalid TYCHO_TVL_THRESHOLD value: {}. Must be a valid number", tvl_str),
                    }.into());
                }
            }
        } else {
            tracing::debug!("TYCHO_TVL_THRESHOLD not set, using default: 70.0");
            env::set_var("TYCHO_TVL_THRESHOLD", "70.0");
        }

        // Validate TYCHO_MIN_PROFIT_BPS if set
        if let Ok(profit_str) = env::var("TYCHO_MIN_PROFIT_BPS") {
            match profit_str.parse::<u64>() {
                Ok(profit) if profit <= 10000 => {
                    tracing::debug!(min_profit_bps = profit, "Valid TYCHO_MIN_PROFIT_BPS value");
                }
                Ok(profit) => {
                    return Err(BundleError::InvalidConfiguration {
                        message: format!("TYCHO_MIN_PROFIT_BPS must be <= 10000 (100%), got: {}", profit),
                    }.into());
                }
                Err(_) => {
                    return Err(BundleError::InvalidConfiguration {
                        message: format!("Invalid TYCHO_MIN_PROFIT_BPS value: {}. Must be a valid integer", profit_str),
                    }.into());
                }
            }
        } else {
            tracing::debug!("TYCHO_MIN_PROFIT_BPS not set, using default: 100");
            env::set_var("TYCHO_MIN_PROFIT_BPS", "100");
        }

        // Validate TYCHO_SLIPPAGE_BPS if set
        if let Ok(slippage_str) = env::var("TYCHO_SLIPPAGE_BPS") {
            match slippage_str.parse::<u64>() {
                Ok(slippage) if slippage <= 10000 => {
                    tracing::debug!(slippage_bps = slippage, "Valid TYCHO_SLIPPAGE_BPS value");
                }
                Ok(slippage) => {
                    return Err(BundleError::InvalidConfiguration {
                        message: format!("TYCHO_SLIPPAGE_BPS must be <= 10000 (100%), got: {}", slippage),
                    }.into());
                }
                Err(_) => {
                    return Err(BundleError::InvalidConfiguration {
                        message: format!("Invalid TYCHO_SLIPPAGE_BPS value: {}. Must be a valid integer", slippage_str),
                    }.into());
                }
            }
        } else {
            tracing::debug!("TYCHO_SLIPPAGE_BPS not set, using default: 50");
            env::set_var("TYCHO_SLIPPAGE_BPS", "50");
        }

        // Validate TYCHO_BRIBE_PERCENTAGE if set
        if let Ok(bribe_str) = env::var("TYCHO_BRIBE_PERCENTAGE") {
            match bribe_str.parse::<u64>() {
                Ok(bribe) if bribe <= 100 => {
                    tracing::debug!(bribe_percentage = bribe, "Valid TYCHO_BRIBE_PERCENTAGE value");
                }
                Ok(bribe) => {
                    return Err(BundleError::InvalidConfiguration {
                        message: format!("TYCHO_BRIBE_PERCENTAGE must be <= 100, got: {}", bribe),
                    }.into());
                }
                Err(_) => {
                    return Err(BundleError::InvalidConfiguration {
                        message: format!("Invalid TYCHO_BRIBE_PERCENTAGE value: {}. Must be a valid integer", bribe_str),
                    }.into());
                }
            }
        } else {
            tracing::debug!("TYCHO_BRIBE_PERCENTAGE not set, using default: 99");
            env::set_var("TYCHO_BRIBE_PERCENTAGE", "99");
        }

        // Validate TYCHO_EXECUTOR_PRIVATE_KEY if set
        if let Ok(key_str) = env::var("TYCHO_EXECUTOR_PRIVATE_KEY") {
            Self::parse_and_validate_private_key(&key_str, "TYCHO_EXECUTOR_PRIVATE_KEY")?;
            tracing::debug!("TYCHO_EXECUTOR_PRIVATE_KEY validated successfully");
        }

        // Validate TYCHO_FLASHBOTS_IDENTITY_KEY if set
        if let Ok(key_str) = env::var("TYCHO_FLASHBOTS_IDENTITY_KEY") {
            Self::parse_and_validate_private_key(&key_str, "TYCHO_FLASHBOTS_IDENTITY_KEY")?;
            tracing::debug!("TYCHO_FLASHBOTS_IDENTITY_KEY validated successfully");
        }

        // Validate TYCHO_RPC_URL if set
        if let Ok(rpc_url) = env::var("TYCHO_RPC_URL") {
            if rpc_url.is_empty() {
                return Err(BundleError::InvalidConfiguration {
                    message: "TYCHO_RPC_URL cannot be empty".to_string(),
                }.into());
            }
            // Basic URL validation
            if url::Url::parse(&rpc_url).is_err() {
                return Err(BundleError::InvalidConfiguration {
                    message: format!("Invalid TYCHO_RPC_URL format: {}", rpc_url),
                }.into());
            }
            tracing::debug!(rpc_url = rpc_url, "Valid TYCHO_RPC_URL value");
        }

        // Validate TYCHO_API_KEY if set
        if let Ok(api_key) = env::var("TYCHO_API_KEY") {
            if api_key.is_empty() {
                return Err(BundleError::InvalidConfiguration {
                    message: "TYCHO_API_KEY cannot be empty".to_string(),
                }.into());
            }
            tracing::debug!("TYCHO_API_KEY validated successfully");
        }

        tracing::debug!("All CLI-specific environment variables validated successfully");
        Ok(())
    }

    /// Parse and validate a private key from a string
    fn parse_and_validate_private_key(key_str: &str, var_name: &str) -> Result<PrivateKeySigner> {
        // Remove 0x prefix if present
        let clean_key = key_str.trim_start_matches("0x");
        
        // Validate key format (should be 64 hex characters)
        if clean_key.len() != 64 {
            return Err(BundleError::InvalidPrivateKey {
                message: format!("{} must be 64 hex characters (32 bytes)", var_name),
            }.into());
        }

        // Validate hex characters
        if !clean_key.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(BundleError::InvalidPrivateKey {
                message: format!("{} contains invalid hex characters", var_name),
            }.into());
        }

        // Parse the private key
        PrivateKeySigner::from_str(clean_key).map_err(|e| {
            BundleError::InvalidPrivateKey {
                message: format!("Failed to parse {}: {}", var_name, e),
            }.into()
        })
    }

    /// Parse and validate an Ethereum address from a string
    fn parse_and_validate_address(address_str: &str, var_name: &str) -> Result<alloy::primitives::Address> {
        crate::utils::string_to_h160(address_str).map_err(|e| {
            BundleError::InvalidConfiguration {
                message: format!("Failed to parse {}: {}", var_name, e),
            }.into()
        })
    }

    /// Validate relayer URLs
    fn validate_relayer_urls(urls: &[String]) -> Result<()> {
        if urls.is_empty() {
            return Err(BundleError::InvalidConfiguration {
                message: "At least one relayer URL must be configured".to_string(),
            }.into());
        }

        for url in urls {
            if !url.starts_with("https://") {
                return Err(BundleError::InvalidConfiguration {
                    message: format!("Relayer URL must use HTTPS: {}", url),
                }.into());
            }

            // Basic URL validation
            if url::Url::parse(url).is_err() {
                return Err(BundleError::InvalidConfiguration {
                    message: format!("Invalid relayer URL format: {}", url),
                }.into());
            }
        }

        Ok(())
    }

    /// Get the relayer URLs
    pub fn relayer_urls(&self) -> &[String] {
        &self.relayer.urls
    }

    /// Get the flashbots identity signer if configured
    pub fn flashbots_identity(&self) -> Option<&PrivateKeySigner> {
        self.security.flashbots_identity.as_ref()
    }

    /// Get the executor signer
    pub fn executor_signer(&self) -> &PrivateKeySigner {
        &self.security.executor_key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::Mutex;

    // Use a mutex to ensure tests don't interfere with each other's environment variables
    static TEST_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_config_from_env_missing_executor_key() {
        let _guard = TEST_MUTEX.lock().unwrap();
        
        // Clear environment
        env::remove_var("TYCHO_EXECUTOR_PRIVATE_KEY");
        env::remove_var("FLASHBOTS_IDENTITY_KEY");
        env::remove_var("BRIBE_PERCENTAGE");
        
        let result = ArbitrageConfig::from_env("ethereum");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("TYCHO_EXECUTOR_PRIVATE_KEY"));
    }

    #[test]
    fn test_config_from_env_invalid_private_key() {
        let _guard = TEST_MUTEX.lock().unwrap();
        
        // Clear any existing environment variables that might interfere
        env::remove_var("TYCHO_EXECUTOR_PRIVATE_KEY");
        env::remove_var("FLASHBOTS_IDENTITY_KEY");
        env::remove_var("BRIBE_PERCENTAGE");
        
        env::set_var("TYCHO_EXECUTOR_PRIVATE_KEY", "invalid_key");
        
        let result = ArbitrageConfig::from_env("ethereum");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("64 hex characters"));
        
        env::remove_var("TYCHO_EXECUTOR_PRIVATE_KEY");
    }

    #[test]
    fn test_config_from_env_valid() {
        let _guard = TEST_MUTEX.lock().unwrap();
        
        // Clear environment first
        env::remove_var("TYCHO_EXECUTOR_PRIVATE_KEY");
        env::remove_var("FLASHBOTS_IDENTITY_KEY");
        env::remove_var("BRIBE_PERCENTAGE");
        env::remove_var("REQUIRE_PROFITABLE");
        
        let test_key = "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
        env::set_var("TYCHO_EXECUTOR_PRIVATE_KEY", test_key);
        
        let result = ArbitrageConfig::from_env("ethereum");
        assert!(result.is_ok(), "Config creation failed: {:?}", result.err());
        
        let config = result.unwrap();
        assert_eq!(config.chain_id, 1);
        assert_eq!(config.bribe_percentage, 50);
        
        env::remove_var("TYCHO_EXECUTOR_PRIVATE_KEY");
    }

    #[test]
    fn test_config_for_testing() {
        let config = ArbitrageConfig::for_testing("ethereum").unwrap();
        assert_eq!(config.chain_id, 1);
        assert!(config.security.flashbots_identity.is_some());
        assert_eq!(config.bribe_percentage, 50);
    }

    #[test]
    fn test_validate_relayer_urls() {
        // Valid URLs
        let valid_urls = vec![
            "https://relay.flashbots.net".to_string(),
            "https://rpc.titanbuilder.xyz".to_string(),
        ];
        assert!(ArbitrageConfig::validate_relayer_urls(&valid_urls).is_ok());

        // Invalid URL (not HTTPS)
        let invalid_urls = vec!["http://insecure.com".to_string()];
        assert!(ArbitrageConfig::validate_relayer_urls(&invalid_urls).is_err());

        // Empty URLs
        let empty_urls = vec![];
        assert!(ArbitrageConfig::validate_relayer_urls(&empty_urls).is_err());
    }

    #[test]
    fn test_parse_private_key_with_0x_prefix() {
        let key_with_prefix = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
        let result = ArbitrageConfig::parse_and_validate_private_key(key_with_prefix, "TEST_KEY");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_private_key_validation() {
        // Test various invalid key formats
        let invalid_keys = vec![
            ("", "empty key"),
            ("123", "too short"),
            ("1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef12345", "too long"),
            ("1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdefgg", "invalid hex"),
        ];

        for (key, description) in invalid_keys {
            let result = ArbitrageConfig::parse_and_validate_private_key(key, "TEST_KEY");
            assert!(result.is_err(), "Expected error for {}: {}", description, key);
        }
    }
}
