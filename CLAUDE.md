# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Development Commands

```bash
cargo build                                      # Dev build
cargo build --release                            # Release build
cargo clippy -- -D warnings                      # Lint (must pass clean)
cargo fmt                                        # Format code
cargo fmt --check                                # Verify formatting
cargo test                                       # Unit + CLI integration tests
cargo test --features integration-tests          # Fork-based integration tests (requires POLYGON_RPC_URL)
RUST_LOG=debug cargo run -- ...                  # Run with debug logging
```

Tests: `cargo test` runs unit tests and CLI integration tests. `cargo test --features integration-tests` runs
chain fork tests using an Anvil fork + mock CLOB server (requires `POLYGON_RPC_URL` env var pointing to a
Polygon RPC endpoint).

## Architecture

Rust CLI (edition 2024) for trading on Polymarket's CLOB API. Supports AWS KMS signing via `--kms-key-id` or local private key signing via `--private-key`.

### Auth Split

Commands are split by authentication requirement:
- **Unauthenticated** (`markets`, `prices`, `dry-run`): Create `Client<Unauthenticated>` — no KMS needed
- **Authenticated** (`orders`, `account`): Require `--kms-key-id` / `POLYMARKET_KMS_KEY_ID` or `--private-key` / `POLYMARKET_PRIVATE_KEY`, create `Client<Authenticated<Normal>>` via EIP-712 L1 auth + L2 HMAC

This is enforced at compile time by the SDK's type-state pattern on `Client<S>`.

### Module Layout

- `cli.rs` — Clap derive definitions for all commands and args
- `signer.rs` — Signer construction (`AnySigner` enum: AWS KMS or local private key)
- `client.rs` — Polymarket SDK client constructors (unauth + auth)
- `output.rs` — Output formatting (`--json` for machine-readable, tables for humans)
- `commands/` — One file per command group, each with standalone async functions
- `dry_run/db.rs` — SQLite database for dry-run trades and balance state
- `dry_run/portfolio.rs` — Position aggregation and P&L computation
- `main.rs` — Tracing init, CLI parse, auth routing, command dispatch, error handling

### Key Dependencies

- `polymarket-client-sdk` (feature `clob`) — Polymarket CLOB client, order signing, market data
- `alloy` (feature `signer-aws`) — AWS KMS EIP-712 signing
- `aws-config` + `aws-sdk-kms` — AWS credential chain resolution
- `rusqlite` (feature `bundled`) — SQLite for dry-run state persistence

### Error Handling

Errors propagate via `anyhow::Result` to `main()`, which formats them (JSON or stderr) and exits non-zero. Command functions must NOT swallow errors — always use `?` to propagate.

## Environment Variables

- `POLYMARKET_KMS_KEY_ID` — AWS KMS key ID (for orders/account, mutually exclusive with private key)
- `POLYMARKET_PRIVATE_KEY` — Hex-encoded private key (for orders/account, mutually exclusive with KMS)
- `POLYMARKET_CLOB_HOST` — CLOB API host (default: `https://clob.polymarket.com`)
- `POLYGON_RPC_URL` — Polygon RPC endpoint (only for integration tests)
- `RUST_LOG` — Tracing log level filter
- Standard AWS credential env vars (`AWS_ACCESS_KEY_ID`, `AWS_PROFILE`, etc.)

## Agent Skills

Skills in `.claude/skills/` teach agents how to use the CLI for Polymarket trading:

- **market-research** — Search, browse, and analyze markets and prices (no auth)
- **paper-trading** — Simulate trades with the dry-run system (no auth, no real money)
- **portfolio** — View balances, positions, and P&L (mixed auth)
- **order-management** — List and cancel open orders (requires auth)
- **trade-execution** — Place real limit/market orders (requires auth, user confirmation mandatory)

All skills instruct agents to use `--json` for structured output. Trade execution requires explicit user confirmation before every order. When intent is ambiguous, agents should default to paper trading.

## Worktrees

Use `.worktrees/` for git worktrees (already in `.gitignore`).
