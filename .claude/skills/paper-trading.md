# Paper Trading (Dry-Run)

Simulate trades on Polymarket without spending real money. Uses a local SQLite database to track positions, balances, and P&L against live market prices.

## When to Use

Use this skill when the user wants to:
- Practice or learn Polymarket trading
- Test a trading strategy before committing real funds
- Simulate a portfolio of prediction market positions
- Track hypothetical P&L over time
- **Default to this skill** when intent is ambiguous between live and paper trading

## Setup

```bash
# Initialize or reset with a starting balance (default $1000)
polymarket-trader dry-run reset --json

# Start with a custom balance
polymarket-trader dry-run reset --balance 5000 --json
```

This clears all existing paper trades and sets a fresh starting balance. The state persists in `~/.polymarket/dry-run.db` between sessions.

## Placing Paper Trades

### Limit Orders

Simulated limit orders fill immediately at the current midpoint price (not your limit price). The limit price serves as a sanity check — the order is rejected if the midpoint has moved beyond it.

```bash
# Buy 100 shares of "Yes" on inflation-2026 at up to $0.65 each
polymarket-trader dry-run limit inflation-2026 buy 0.65 100 --json

# Sell 50 shares
polymarket-trader dry-run limit inflation-2026 sell 0.60 50 --json

# Target a specific outcome in multi-outcome markets
polymarket-trader dry-run limit president-2028 buy 0.30 200 --outcome "Harris" --json
```

### Market Orders

Specify USDC amount instead of shares. Shares are calculated from the current midpoint.

```bash
# Buy $100 worth of shares at current price
polymarket-trader dry-run market inflation-2026 buy 100 --json

# Sell $50 worth
polymarket-trader dry-run market inflation-2026 sell 50 --json
```

### Closing Positions

```bash
# Close entire position at current market price
polymarket-trader dry-run close inflation-2026 --json

# Partial close — sell only 50 shares
polymarket-trader dry-run close inflation-2026 --size 50 --json
```

### Cancelling Trades

```bash
# Remove a specific trade (reverses its balance impact)
polymarket-trader dry-run cancel <trade-id> --json
```

The trade ID is returned in the JSON output when placing a trade.

## Viewing State

### Positions

```bash
# Net positions by token (aggregated from all trades)
polymarket-trader dry-run positions --json
```

Shows: token ID, market info, side (long/short), net size, average price, total cost.

### Trade History

```bash
# Recent trades (default 25)
polymarket-trader dry-run trades --json

# More history
polymarket-trader dry-run trades --limit 100 --json
```

### P&L Report

```bash
# Unrealized P&L with current live prices
polymarket-trader dry-run pnl --json
```

Shows per-position unrealized P&L and total portfolio performance vs starting balance.

### Full Portfolio

```bash
# Combined view: positions + cash + totals
polymarket-trader dry-run portfolio --json
```

The most comprehensive view — includes positions with current values, cash balance, total portfolio value, and overall P&L.

## Workflow: Testing a Trading Strategy

1. **Reset** with a starting balance:
   ```bash
   polymarket-trader dry-run reset --balance 1000 --json
   ```

2. **Research** markets (see market-research skill):
   ```bash
   polymarket-trader markets trending --json
   polymarket-trader prices midpoint <slug> --json
   ```

3. **Enter positions**:
   ```bash
   polymarket-trader dry-run market <slug> buy 100 --json
   ```

4. **Monitor** portfolio over time:
   ```bash
   polymarket-trader dry-run portfolio --json
   polymarket-trader dry-run pnl --json
   ```

5. **Adjust** — close losers, add to winners:
   ```bash
   polymarket-trader dry-run close <slug> --json
   polymarket-trader dry-run market <other-slug> buy 50 --json
   ```

6. **Review** final performance:
   ```bash
   polymarket-trader dry-run pnl --json
   ```

## How It Works

- Trades fill at the **live midpoint price** fetched from Polymarket's CLOB API
- Balance is tracked locally — buys deduct from cash, sells add to cash
- Positions are aggregated across all trades for the same token
- P&L is computed against live prices, so it updates as markets move
- All state persists in SQLite at `~/.polymarket/dry-run.db`

## Error Handling

- **"Insufficient balance"** — not enough cash for the trade; check with `dry-run portfolio --json`
- **"No position to close"** — no net position exists for that market
- **"Insufficient position size"** — trying to close more shares than held
- **"Could not fetch midpoint"** — market may be inactive or slug is wrong; verify with `markets show <slug> --json`

## Notes

- No KMS key or wallet needed — all paper trading is local
- Midpoint-based fills are a simplification; real trading has slippage and spread costs
- The dry-run system does NOT simulate order book dynamics or partial fills
- Reset is destructive — all trade history is lost; there is no undo
- Always use `--json` flag when parsing output programmatically
