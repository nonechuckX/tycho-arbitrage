use clap::Parser;
use tycho_common::Bytes;
use tycho_common::models::Chain;
use tycho_atomic_arbitrage::errors::Result;
use std::str::FromStr;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
    #[clap(long, env = "TYCHO_CHAIN", default_value = "ethereum", help = "Target blockchain (e.g., ethereum, base)")]
    pub chain: String,

    #[clap(long, env = "TYCHO_RPC_URL", help = "RPC URL for on-chain interaction")]
    pub rpc_url: String,

    #[clap(long, env = "TYCHO_API_KEY", help = "Tycho API key")]
    pub tycho_api_key: String,

    #[clap(long, value_delimiter = ',', help = "Comma-separated list of token symbols or addresses to start cycles from (e.g., WETH,USDC)")]
    pub start_tokens: Vec<String>,

    #[clap(long, value_delimiter = ',', help = "Comma-separated list of optimization tolerance percentages, one for each start token (e.g., 1.0,0.5). Defaults to 1.0 for each start token if not provided.")]
    pub optimization_tolerances: Vec<f64>,

    #[clap(long, env = "TYCHO_EXECUTOR_PRIVATE_KEY", help = "Private key for the executor EOA")]
    pub executor_private_key: String,

    #[clap(long, env = "TYCHO_TVL_THRESHOLD", default_value_t = 70.0, help = "Minimum TVL for pools to consider")]
    pub tvl_threshold: f64,

    #[clap(long, env = "TYCHO_MIN_PROFIT_BPS", default_value_t = 100, help = "Minimum profit in BPS of spot price product to consider for optimization")]
    pub min_profit_bps: u64,

    #[clap(long, env = "TYCHO_SLIPPAGE_BPS", default_value_t = 500, help = "Slippage tolerance in BPS for trades")]
    pub slippage_bps: u64,

    #[clap(long, env = "TYCHO_FLASHBOTS_IDENTITY_KEY", help = "Private key for Flashbots authentication")]
    pub flashbots_identity: Option<String>,

    #[clap(long, env = "TYCHO_BRIBE_PERCENTAGE", default_value_t = 99, help = "Bribe percentage of expected profit")]
    pub bribe_percentage: u64,
}

const WETH_ADDRESSES: &[(&str, &str)] = &[
    ("ethereum", "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"),
    ("base", "0x4200000000000000000000000000000000000006"),
    ("unichain", "0x4200000000000000000000000000000000000006"),
];

const USDC_ADDRESSES: &[(&str, &str)] = &[
    ("ethereum", "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"),
    ("base", "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913"),
    ("unichain", "0x078D782b760474a361dDA0AF3839290b0EF57AD6"),
];

const WBTC_ADDRESSES: &[(&str, &str)] = &[
    ("ethereum", "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599"),
    ("base", "0x0555e30da8f98308edb960aa94c0db47230d2b9c"),
    ("unichain", "0x0555E30da8f98308EdB960aa94C0Db47230d2B9c"),
];

impl Args {
    /// Set environment variables from parsed CLI arguments
    /// This ensures that the config module can find all required environment variables
    pub fn set_environment_variables(&self) -> Result<()> {
        use std::env;
        
        tracing::debug!("Setting environment variables from CLI arguments");
        
        // Set required environment variables from CLI arguments
        env::set_var("TYCHO_EXECUTOR_PRIVATE_KEY", &self.executor_private_key);
        env::set_var("TYCHO_CHAIN", &self.chain);
        env::set_var("TYCHO_RPC_URL", &self.rpc_url);
        env::set_var("TYCHO_API_KEY", &self.tycho_api_key);
        
        // Set RPC_URL as an alias to TYCHO_RPC_URL for external dependencies that expect it
        // This maintains compatibility while keeping TYCHO_RPC_URL as the primary source
        env::set_var("RPC_URL", &self.rpc_url);
        env::set_var("TYCHO_TVL_THRESHOLD", &self.tvl_threshold.to_string());
        env::set_var("TYCHO_MIN_PROFIT_BPS", &self.min_profit_bps.to_string());
        env::set_var("TYCHO_SLIPPAGE_BPS", &self.slippage_bps.to_string());
        env::set_var("TYCHO_BRIBE_PERCENTAGE", &self.bribe_percentage.to_string());
        
        // Set both TYCHO_ prefixed and non-prefixed versions for config compatibility
        env::set_var("BRIBE_PERCENTAGE", &self.bribe_percentage.to_string());
        
        // Set optional flashbots identity key if provided
        if let Some(ref flashbots_key) = self.flashbots_identity {
            env::set_var("TYCHO_FLASHBOTS_IDENTITY_KEY", flashbots_key);
            env::set_var("FLASHBOTS_IDENTITY_KEY", flashbots_key);
        }
        
        tracing::info!(
            chain = %self.chain,
            rpc_url = %self.rpc_url,
            tvl_threshold = %self.tvl_threshold,
            min_profit_bps = %self.min_profit_bps,
            slippage_bps = %self.slippage_bps,
            bribe_percentage = %self.bribe_percentage,
            has_flashbots_identity = self.flashbots_identity.is_some(),
            "Environment variables set from CLI arguments"
        );
        
        Ok(())
    }

    pub fn with_defaults(mut self) -> Result<Self> {
        // Handle default WETH token first if no start tokens provided
        if self.start_tokens.is_empty() {
            let weth_address = Self::get_weth_address(&self.chain)?;
            self.start_tokens.push(weth_address);
        }

        // Now handle optimization tolerances based on final start_tokens count
        use std::cmp::Ordering;
        match self.start_tokens.len().cmp(&self.optimization_tolerances.len()) {
            Ordering::Greater => {
                // More start tokens than tolerances - extend with 1.0 defaults
                let diff = self.start_tokens.len() - self.optimization_tolerances.len();
                self.optimization_tolerances.extend(vec![1.0; diff]);
            }
            Ordering::Less => {
                // More tolerances than start tokens - truncate to match
                self.optimization_tolerances.truncate(self.start_tokens.len());
            }
            Ordering::Equal => {
                // Equal counts - no change needed
            }
        }

        // Handle completely empty optimization_tolerances (fallback safety)
        if self.optimization_tolerances.is_empty() && !self.start_tokens.is_empty() {
            self.optimization_tolerances = vec![1.0; self.start_tokens.len()];
        }

        Ok(self)
    }

    fn get_weth_address(chain: &str) -> Result<String> {
        WETH_ADDRESSES
            .iter()
            .find(|(c, _)| *c == chain)
            .map(|(_, addr)| addr.to_string())
            .ok_or_else(|| {
                anyhow::anyhow!("Default WETH address not set for chain: {}", chain).into()
            })
    }

    fn get_token_address(token_symbol: &str, chain: &str) -> Result<Bytes> {
        let address_str = match token_symbol {
            "WETH" => WETH_ADDRESSES
                .iter()
                .find(|(c, _)| *c == chain)
                .map(|(_, addr)| *addr),
            "USDC" => USDC_ADDRESSES
                .iter()
                .find(|(c, _)| *c == chain)
                .map(|(_, addr)| *addr),
            "WBTC" => WBTC_ADDRESSES
                .iter()
                .find(|(c, _)| *c == chain)
                .map(|(_, addr)| *addr),
            _ => None,
        };

        match address_str {
            Some(addr) => Bytes::from_str(addr).map_err(|e| anyhow::anyhow!("Invalid address format: {}", e).into()),
            None => Err(anyhow::anyhow!("Token {} not supported on chain {}", token_symbol, chain).into()),
        }
    }


    pub fn native_token(&self) -> Result<Bytes> {
        Self::get_token_address("WETH", &self.chain)
    }

    pub fn start_tokens(&self) -> Result<Vec<Bytes>> {
        let mut source_tokens = Vec::new();

        if self.start_tokens.is_empty() {
            source_tokens.push(self.native_token()?);
        } else {
            for token in self.start_tokens.iter() {
                if token.starts_with("0x") && token.len() == 42 {
                    // Handle raw addresses
                    match Bytes::from_str(token) {
                        Ok(bytes) => source_tokens.push(bytes),
                        Err(e) => {
                            tracing::warn!(
                                token = token,
                                error = %e,
                                "Failed to parse raw token address, skipping"
                            );
                        }
                    }
                } else {
                    // Handle token symbols
                    match Self::get_token_address(token, &self.chain) {
                        Ok(bytes) => source_tokens.push(bytes),
                        Err(e) => {
                            tracing::warn!(
                                token = token,
                                chain = self.chain,
                                error = %e,
                                "Failed to resolve token symbol, skipping"
                            );
                        }
                    }
                }
            }
        }

        if source_tokens.is_empty() {
            return Err(anyhow::anyhow!("No valid start tokens found").into());
        }

        Ok(source_tokens)
    }

    pub fn tycho_url(&self) -> Result<String> {
        use tycho_atomic_arbitrage::utils::get_default_tycho_url;
        
        let chain = match self.chain.as_str() {
            "ethereum" => Chain::Ethereum,
            "base" => Chain::Base,
            "unichain" => Chain::Unichain,
            _ => return Err(anyhow::anyhow!("Unsupported chain: {}", self.chain).into()),
        };
        
        get_default_tycho_url(&chain)
            .ok_or_else(|| anyhow::anyhow!("No default Tycho URL configured for chain: {}", self.chain).into())
    }
}

pub fn parse_cli_args() -> Result<Args> {
    let args = Args::parse().with_defaults()?;
    
    // Set environment variables from CLI arguments so that config module can find them
    args.set_environment_variables()?;
    
    Ok(args)
}
