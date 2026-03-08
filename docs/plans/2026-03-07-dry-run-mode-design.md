# Dry-Run Mode - Design Document

## Goal

Add a `dry-run` subcommand group that simulates trades using real market prices without executing on-chain. Tracks a virtual portfolio in a local SQLite database so an agent can evaluate strategy profitability over time.

## Architecture

Standalone `dry-run` subcommand namespace. No KMS auth required — uses unauthenticated CLOB client to fetch real midpoint prices. Simulated trades record instant fills at the current midpoint. Portfolio state persists in SQLite at `~/.polymarket/dry-run.db`.

## CLI Commands

```
polymarket-trader dry-run <COMMAND>

Commands:
  limit      <token-id> <side> <price> <size>   Simulated limit order (fills at midpoint)
  market     <token-id> <side> <amount>          Simulated market order (fills at midpoint)
  cancel     <trade-id>                          Remove a simulated trade
  positions                                      Show current simulated positions
  trades     [--limit N]                         Show simulated trade history
  pnl                                            Show P&L (current midpoints vs fill prices)
  reset      [--balance N]                       Clear state, optionally set starting balance
```

- No `--kms-key-id` required
- `--json` flag works on all dry-run commands
- Both `limit` and `market` fetch real midpoint and record instant fill
- `pnl` fetches live midpoints to calculate unrealized P&L

## Data Model

### SQLite Schema (`~/.polymarket/dry-run.db`)

```sql
CREATE TABLE trades (
    id TEXT PRIMARY KEY,
    token_id TEXT NOT NULL,
    side TEXT NOT NULL,
    price TEXT NOT NULL,
    size TEXT NOT NULL,
    cost TEXT NOT NULL,
    timestamp TEXT NOT NULL
);

CREATE TABLE state (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

- Decimals stored as TEXT for precision
- `state` table holds `starting_balance` (default "1000.00") and `balance`
- Positions derived at query time via GROUP BY on trades
- `reset` drops and recreates tables

### Balance Tracking

- Starting balance defaults to 1000 USDC
- Buying deducts `price * size` from balance
- Selling adds `price * size` to balance
- Orders rejected if insufficient simulated balance

### P&L Calculation

1. Aggregate positions: GROUP BY token_id with net size and weighted average fill price
2. Fetch current midpoint from CLOB for each position
3. Unrealized P&L per position = (current_midpoint - avg_fill) * net_size (sign-adjusted for side)
4. Total P&L = remaining_balance - starting_balance + unrealized value of open positions

## Module Structure

```
src/
├── commands/
│   └── dry_run.rs         # All dry-run subcommands
├── dry_run/
│   ├── mod.rs             # Re-exports
│   ├── db.rs              # SQLite schema, CRUD operations
│   └── portfolio.rs       # Position aggregation, P&L calculation
├── cli.rs                 # Add DryRun variant to Command enum
└── main.rs                # Add dry-run dispatch (unauthenticated path)
```

## Dependencies

```toml
rusqlite = { version = "0.34", features = ["bundled"] }
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
dirs = "6"
```

## Integration

- `dry-run` routes through the unauthenticated client path in main.rs (same as markets/prices)
- No changes to existing command modules
- CLOB client used read-only for midpoint fetches
