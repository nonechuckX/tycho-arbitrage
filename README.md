# Triangle - Tycho Atomic Arbitrage Bot
<img width="1494" height="444" alt="image" src="https://github.com/user-attachments/assets/bd23de6b-da76-4605-9bcc-7d345a54ecfc" />

Triangle is a high-performance atomic arbitrage bot built with Rust and the [Tycho library](https://docs.propellerheads.xyz/tycho). This project demonstrates real-time arbitrage opportunity detection and execution across multiple decentralized exchanges.

ℹ️ **Project Background:** Built for a Tycho Application bounty – [TAP 4](https://github.com/propeller-heads/tycho-x/blob/main/TAP-4.md). Join developers using Tycho in [tycho.build](https://t.me/+B4CNQwv7dgIyYTJl). 

## Features

- **Real-time DEX Monitoring**: Connects to Tycho's live data feed for block-by-block pool state updates
- **Graph-based Pathfinding**: Builds token trading graphs to efficiently discover arbitrage cycles
- **Atomic Bundle Execution**: Submits transaction bundles to MEV relayers for atomic execution (only Ethereum mainnet)
- **Multi-chain Support**: Works on Ethereum, Base, and Unichain networks
- **Configurable Risk Management**: Customizable profit thresholds and slippage protection

## Architecture

The system follows a modular architecture with clear separation of concerns:

```
Tycho Stream → Trading Graph → Path Discovery → Simulation → Bundle Submission
```

- **`src/graph/`**: Token trading graph with nodes (tokens) and edges (liquidity pools)
- **`src/path/`**: Arbitrage path discovery, validation, and optimization
- **`src/simulation/`**: Transaction simulation and validation engine
- **`src/bundle/`**: Bundle creation and submission to MEV relayers
- **`src/config/`**: Secure configuration management and validation

## Quick Start

### Prerequisites

- Rust 1.70+ and Cargo
- Tycho API key and endpoint access. If you don't have an API key, you can use
  ```
  TYCHO_API_KEY=sampletoken
  ```
- Ethereum-compatible RPC URL which supports `eth_simulateV1`. For testing, you can use  
  ```
  TYCHO_RPC_URL=https://docs-demo.quiknode.pro/
  ```
  However, higher requests per second (RPS) are required to avoid rate limiting errors. If needed, you can create a free endpoint at dashboard.quicknode.com
- Private key for transaction execution

### Setup

1. Clone the repository:
   ```bash
   git clone <repository-url>
   cd tycho-atomic-arbitrage
   ```

2. Create your configuration:
   ```bash
   cp .env.example .env
   # Edit .env with your API keys and configuration
   ```

3. Run the arbitrage bot:
   ```bash
   cargo run --release --example arbitrage-bot
   ```

## Configuration

Essential environment variables:

```bash
# Required
TYCHO_RPC_URL=your_rpc_endpoint
TYCHO_API_KEY=your_tycho_api_key
TYCHO_EXECUTOR_PRIVATE_KEY=your_private_key_without_0x

# Optional
TYCHO_CHAIN=ethereum                    # ethereum, base, unichain
TYCHO_TVL_THRESHOLD=70.0               # Minimum pool TVL in native currency
TYCHO_MIN_PROFIT_BPS=100               # Minimum profit in basis points
TYCHO_BRIBE_PERCENTAGE=99              # MEV bribe percentage
```

## Example Usage

Run on Ethereum mainnet with custom parameters:
```bash
cargo run --release --example arbitrage-bot -- \
  --chain ethereum \
  --start-tokens WETH,USDC \
  --tvl-threshold 100 \
  --min-profit-bps 50
```

## Output

The bot continuously monitors for arbitrage opportunities and logs:
- Block updates and pool state changes
- Discovered arbitrage paths and profitability
- Transaction simulation results
- Bundle submission status

For detailed configuration options and advanced usage, see the [examples README](examples/arbitrage-bot/README.md).

## License

This project is provided as-is for educational and demonstration purposes.
