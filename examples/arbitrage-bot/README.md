# Atomic Arbitrage Bot Example

This example demonstrates a complete atomic arbitrage bot that monitors decentralized exchanges for price discrepancies and executes profitable trades through MEV bundles.

## What It Does

The bot continuously:
1. **Monitors** live pool states from Tycho's data feed
2. **Discovers** arbitrage cycles using graph-based pathfinding
3. **Simulates** transactions to validate profitability
4. **Executes** profitable opportunities via MEV relayers

## Configuration

### Required Environment Variables

```bash
TYCHO_RPC_URL=your_ethereum_rpc_endpoint
TYCHO_API_KEY=your_tycho_api_key
TYCHO_EXECUTOR_PRIVATE_KEY=your_private_key_without_0x_prefix
```

### Optional Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `TYCHO_CHAIN` | `ethereum` | Target blockchain (ethereum, base, unichain) |
| `TYCHO_TVL_THRESHOLD` | `70.0` | Minimum pool TVL in native currency |
| `TYCHO_MIN_PROFIT_BPS` | `100` | Minimum profit threshold (basis points) |
| `TYCHO_SLIPPAGE_BPS` | `500` | Slippage tolerance (basis points) |
| `TYCHO_BRIBE_PERCENTAGE` | `99` | MEV bribe percentage (0-100) |
| `TYCHO_FLASHBOTS_IDENTITY_KEY` | - | Flashbots identity key (optional) |

## Usage

### Basic Usage

```bash
cargo run --release --example arbitrage-bot
```

### Custom Configuration

```bash
cargo run --release --example arbitrage-bot -- \
  --chain ethereum \
  --start-tokens WETH,USDC,WBTC \
  --tvl-threshold 100 \
  --min-profit-bps 50 \
  --slippage-bps 300
```