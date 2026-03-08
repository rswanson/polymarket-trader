# polymarket-trader

A Rust CLI for trading on [Polymarket](https://polymarket.com) prediction markets. Designed as infrastructure for automated trading agents, with AWS KMS wallet signing (no private keys on disk).

## Features

- **Market data** — list markets, view prices, spreads, and order books
- **Order management** — place limit and market orders, cancel orders
- **Account info** — check balances and trade history
- **AWS KMS signing** — wallet keys stay in hardware security modules
- **JSON output** — `--json` flag for machine-readable output (agent-friendly)
- **No auth for read-only** — market data commands work without KMS credentials

## Prerequisites

- Rust 1.88+ (edition 2024)
- AWS credentials configured (via env vars, `~/.aws/credentials`, or IAM role)
- An AWS KMS key for ECDSA signing (secp256k1)

## Installation

```bash
# From source
cargo install --path .

# Or build directly
cargo build --release
# Binary at target/release/polymarket-trader
```

## Configuration

| Environment Variable | Description | Required |
|---|---|---|
| `POLYMARKET_KMS_KEY_ID` | AWS KMS key ID or ARN | For orders/account commands |
| `POLYMARKET_CLOB_HOST` | CLOB API host (default: `https://clob.polymarket.com`) | No |
| `RUST_LOG` | Log level (`debug`, `info`, `warn`, `error`) | No |
| `AWS_PROFILE` / `AWS_ACCESS_KEY_ID` / etc. | AWS credentials | For orders/account commands |

All config can also be passed as CLI flags (e.g. `--kms-key-id`, `--clob-host`).

## Usage

### Market Data (no auth required)

```bash
# List active markets
polymarket-trader markets list --limit 10

# Show details for a specific market
polymarket-trader markets show <condition-id>

# Get midpoint price for a token
polymarket-trader prices midpoint <token-id>

# Get bid-ask spread
polymarket-trader prices spread <token-id>

# View full order book
polymarket-trader prices book <token-id>
```

### Trading (requires KMS key)

```bash
# Place a limit order (buy 100 shares at $0.55)
polymarket-trader orders limit <token-id> buy 0.55 100

# Place a market order (spend $50 USDC)
polymarket-trader orders market <token-id> buy 50

# List open orders
polymarket-trader orders list

# Cancel a specific order
polymarket-trader orders cancel <order-id>

# Cancel all orders
polymarket-trader orders cancel-all
```

### Account

```bash
# Check USDC balance
polymarket-trader account balance

# View recent trades
polymarket-trader account trades --limit 10
```

### Dry-Run (Paper Trading)

Simulate trades using real market prices without executing on-chain. No KMS credentials needed.

```bash
# Reset with custom starting balance
polymarket-trader dry-run reset --balance 5000

# Simulate a limit buy (fills at current midpoint)
polymarket-trader dry-run limit <token-id> buy 0.55 100

# Simulate a market buy ($50 USDC)
polymarket-trader dry-run market <token-id> buy 50

# View simulated positions
polymarket-trader dry-run positions

# View trade history
polymarket-trader dry-run trades

# Check P&L (fetches live prices)
polymarket-trader dry-run pnl

# Cancel a simulated trade
polymarket-trader dry-run cancel <trade-id>
```

Portfolio state is stored in `~/.polymarket/dry-run.db` (SQLite).

### JSON Output

Append `--json` to any command for structured output:

```bash
polymarket-trader --json prices midpoint <token-id>
```

## Architecture

The CLI uses [polymarket-client-sdk](https://crates.io/crates/polymarket-client-sdk) for Polymarket's CLOB API and [alloy](https://github.com/alloy-rs/alloy) with `signer-aws` for AWS KMS transaction signing.

Read-only commands (markets, prices) skip authentication entirely. Trading commands authenticate via EIP-712 signing against the CLOB, with ongoing requests using HMAC-SHA256.

## License

MIT
