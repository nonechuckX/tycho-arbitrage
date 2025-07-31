use crate::cli::Args;
use futures::{Stream, StreamExt};
use std::pin::Pin;
use tycho_atomic_arbitrage::errors::Result;
use tycho_simulation::utils::load_all_tokens;
use tycho_simulation::evm::decoder::StreamDecodeError;
use tycho_simulation::evm::engine_db::tycho_db::PreCachedDB;
use tycho_simulation::evm::stream::ProtocolStreamBuilder;
use tycho_simulation::evm::tycho_models::Chain;
use tycho_simulation::protocol::models::BlockUpdate;
use tycho_simulation::tycho_client::feed::component_tracker::ComponentFilter;
use tycho_simulation::evm::protocol::{
    filters::{
        balancer_pool_filter as BalancerPF, curve_pool_filter as CurvePF,
        uniswap_v4_pool_with_hook_filter as UniV4PF,
    },
    pancakeswap_v2::state::PancakeswapV2State,
    uniswap_v2::state::UniswapV2State,
    uniswap_v3::state::UniswapV3State,
    uniswap_v4::state::UniswapV4State,
    vm::state::EVMPoolState,
};

pub struct TychoStream {
    stream: Pin<Box<dyn Stream<Item = std::result::Result<BlockUpdate, StreamDecodeError>> + Send>>,
}

impl TychoStream {
    pub async fn new(args: &Args) -> Result<Self> {
        let tycho_url = args.tycho_url()?;
        
        let chain = match args.chain.as_str() {
            "base" => Chain::Base,
            "unichain" => Chain::Unichain,
            _ => Chain::Ethereum,
        };

        tracing::info!(
            chain = %chain,
            tycho_url = %tycho_url,
            tvl_threshold = args.tvl_threshold,
            "Initializing Tycho stream"
        );

        let tokens = load_all_tokens(
            &tycho_url,
            false,
            Some(&args.tycho_api_key),
            chain.clone(),
            None,
            None,
        )
        .await;

        let tvl_filter = ComponentFilter::with_tvl_range(args.tvl_threshold, args.tvl_threshold);

        let mut stream_builder = ProtocolStreamBuilder::new(&tycho_url, chain.clone());
        stream_builder = match chain {
            Chain::Ethereum => Self::with_ethereum_exchanges(stream_builder, tvl_filter),
            Chain::Base => Self::with_base_exchanges(stream_builder, tvl_filter),
            Chain::Unichain => Self::with_unichain_exchanges(stream_builder, tvl_filter),
            _ => {
                tracing::warn!(chain = %chain, "Chain not fully supported, using minimal configuration");
                stream_builder
            }
        };

        let stream = stream_builder
            .auth_key(Some(args.tycho_api_key.clone()))
            .skip_state_decode_failures(true)
            .set_tokens(tokens)
            .await
            .build()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to build Tycho stream: {}", e))?;

        tracing::info!("Tycho stream initialized successfully");

        Ok(Self {
            stream: Box::pin(stream),
        })
    }

    pub async fn next(&mut self) -> Option<BlockUpdate> {
        match self.stream.next().await {
            Some(Ok(block_update)) => {
                tracing::info!(
                    block_number = block_update.block_number,
                    new_pairs = block_update.new_pairs.len(),
                    removed_pairs = block_update.removed_pairs.len(),
                    state_updates = block_update.states.len(),
                    "Received block update"
                );
                Some(block_update)
            }
            Some(Err(err)) => {
                tracing::error!(error = %err, "Block decode error");
                None
            }
            None => {
                tracing::debug!("Stream ended");
                None
            }
        }
    }

    fn with_ethereum_exchanges(
        stream_builder: ProtocolStreamBuilder,
        tvl_filter: ComponentFilter,
    ) -> ProtocolStreamBuilder {
        stream_builder
            .exchange::<UniswapV2State>("uniswap_v2", tvl_filter.clone(), None)
            .exchange::<UniswapV2State>("sushiswap_v2", tvl_filter.clone(), None)
            .exchange::<PancakeswapV2State>("pancakeswap_v2", tvl_filter.clone(), None)
            .exchange::<UniswapV3State>("uniswap_v3", tvl_filter.clone(), None)
            .exchange::<UniswapV3State>("pancakeswap_v3", tvl_filter.clone(), None)
            .exchange::<EVMPoolState<PreCachedDB>>(
                "vm:balancer_v2",
                tvl_filter.clone(),
                Some(BalancerPF),
            )
            .exchange::<UniswapV4State>("uniswap_v4", tvl_filter.clone(), Some(UniV4PF))
            //.exchange::<EkuboState>("ekubo_v2", tvl_filter.clone(), None)
            .exchange::<EVMPoolState<PreCachedDB>>("vm:curve", tvl_filter.clone(), Some(CurvePF))
    }

    fn with_base_exchanges(
        stream_builder: ProtocolStreamBuilder,
        tvl_filter: ComponentFilter,
    ) -> ProtocolStreamBuilder {
        stream_builder
            .exchange::<UniswapV2State>("uniswap_v2", tvl_filter.clone(), None)
            .exchange::<UniswapV3State>("uniswap_v3", tvl_filter.clone(), None)
            //.exchange::<UniswapV3State>("pancakeswap_v3", tvl_filter.clone(), None)
            //.exchange::<UniswapV4State>("uniswap_v4", tvl_filter.clone(), Some(UniV4PF))
    }

    fn with_unichain_exchanges(
        stream_builder: ProtocolStreamBuilder,
        tvl_filter: ComponentFilter,
    ) -> ProtocolStreamBuilder {
        stream_builder
            .exchange::<UniswapV2State>("uniswap_v2", tvl_filter.clone(), None)
            .exchange::<UniswapV3State>("uniswap_v3", tvl_filter.clone(), None)
            .exchange::<UniswapV3State>("pancakeswap_v3", tvl_filter.clone(), None)
            .exchange::<UniswapV4State>("uniswap_v4", tvl_filter.clone(), Some(UniV4PF))
    }
}

#[cfg(test)]
mod stream_test {
    use crate::stream::TychoStream;
    use dotenv::dotenv;

    #[tokio::test]
    #[ignore]
    async fn test_tycho_stream_new() {
        dotenv().ok();
        let tycho_api_key = std::env::var("TYCHO_API_KEY").unwrap_or_default();
        TychoStream::new(
            "ethereum",
            "https://eth.mainnet.tycho.xyz",
            &tycho_api_key,
            1000.0,
        )
        .await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_tycho_stream_next() {
        dotenv().ok();
        let tycho_api_key = std::env::var("TYCHO_API_KEY").unwrap_or_default();
        let mut stream = TychoStream::new(
            "ethereum",
            "https://eth.mainnet.tycho.xyz",
            &tycho_api_key,
            1000.0,
        )
        .await;

        let next = stream.next().await;
        assert!(next.is_some());
    }
}
