# Polymarket Trader CLI - Design Document

## Goal

A Rust CLI for executing trades and gathering market data on Polymarket, designed as infrastructure for an AI agent to eventually operate trading strategies. Uses AWS KMS for wallet signing (no private keys on disk).

## Architecture

### Tech Stack

- **Language:** Rust (edition 2024)
- **Polymarket SDK:** `polymarket-client-sdk` v0.4 (features: `clob`, `ctf`)
- **Signing:** `alloy` v1.6 with `signer-aws` feature (`AwsSigner` for AWS KMS)
- **AWS:** `aws-config` + `aws-sdk-kms` (default credential chain)
- **CLI:** `clap` (derive macros)
- **Async:** `tokio`
- **Output:** `comfy-table` for human-readable, `serde_json` for `--json` mode
- **Logging:** `tracing` + `tracing-subscriber`

### Chain

Polygon (chain ID 137). Polymarket's CTF Exchange and Conditional Tokens Framework contracts live here.

### Authentication Flow

1. CLI reads KMS key ID from `POLYMARKET_KMS_KEY_ID` env var or `--kms-key-id` flag
2. AWS SDK resolves credentials via default chain (env vars, profiles, IAM roles)
3. `AwsSigner::new(kms_client, key_id, Some(137))` creates the alloy signer
4. `Client::new(url).authentication_builder(&signer).authenticate().await` performs EIP-712 L1 auth against CLOB
5. All subsequent commands use the authenticated client with L2 HMAC auth

### Project Structure

```
polymarket-trader/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI entry point (clap, tokio::main)
│   ├── config.rs             # KMS key ID, CLOB URL, chain config
│   ├── client.rs             # Polymarket client setup + auth
│   ├── commands/
│   │   ├── mod.rs
│   │   ├── markets.rs        # list, search, view markets
│   │   ├── orders.rs         # place, cancel, view orders
│   │   ├── book.rs           # order book, prices, spreads
│   │   └── account.rs        # balance, positions, trades
│   └── output.rs             # Formatting (table/json output)
```

## CLI Commands

All commands support `--json` flag for machine-readable output.

```
polymarket-trader [--json] [--kms-key-id <KEY_ID>] <COMMAND>
```

### Markets

| Command | Description |
|---|---|
| `markets list [--limit N]` | List active markets |
| `markets search <query>` | Search markets by keyword |
| `markets show <condition-id>` | Market details (odds, volume, liquidity) |

### Prices

| Command | Description |
|---|---|
| `prices <condition-id>` | Current price/midpoint |
| `prices spread <condition-id>` | Bid-ask spread |
| `prices book <condition-id>` | Full order book |

### Orders

| Command | Description |
|---|---|
| `orders list [--open\|--all]` | List your orders |
| `orders place limit <condition-id> <side> <price> <size>` | Place limit order |
| `orders place market <condition-id> <side> <size>` | Place market order |
| `orders cancel <order-id>` | Cancel specific order |
| `orders cancel-all [--market <id>]` | Cancel all orders |

- `side` = `yes` or `no`
- Prices in decimal (0.01 - 0.99)
- Sizes in USDC

### Account

| Command | Description |
|---|---|
| `account balance` | USDC balance + allowance |
| `account positions` | Current conditional token positions (on-chain ERC-1155 query) |
| `account trades [--limit N]` | Recent trade history |

## Error Handling

- **Auth failures** (KMS permission denied, invalid key): clear error message with AWS troubleshooting hint
- **Network errors** (CLOB down, RPC unreachable): retry with exponential backoff (3 attempts), then fail with context
- **Order rejections** (insufficient balance, invalid price): display CLOB error message directly
- All errors exit with non-zero status code
- `--json` mode outputs structured error objects

## Output

- **Default:** Human-readable tables via `comfy-table`
- **`--json`:** Structured JSON objects (for agent consumption)
- **Logging:** `RUST_LOG` env var controls verbosity via `tracing-subscriber`

## Key Dependencies (Cargo.toml)

```toml
[dependencies]
polymarket-client-sdk = { version = "0.4", features = ["clob", "ctf"] }
alloy = { version = "1.6", features = ["signer-aws"] }
aws-config = "1.8"
aws-sdk-kms = "1.99"
clap = { version = "4", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
comfy-table = "7"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

## On-Chain Interaction

Polymarket's architecture is hybrid: orders are signed off-chain (EIP-712), matched by CLOB operator, settled on-chain. Direct on-chain interaction is used for:

- Token approvals (USDC allowance to CTF Exchange)
- Querying ERC-1155 conditional token balances (positions)
- Merging tokens (YES + NO -> USDC)
- Redeeming winning tokens after resolution
- On-chain order cancellation (escape hatch)

## Future Considerations (Not in MVP)

- WebSocket market data streams (`ws` feature)
- Automated strategy execution loop
- Portfolio P&L tracking
- Market data export for analysis
- Config file (~/.polymarket/config.toml)
