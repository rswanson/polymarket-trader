# Portfolio & Account

View account balances, positions, trade history, and P&L.

## When to Use

Use this skill when the user wants to:
- Check their USDC balance or token allowances
- View recent trade history (live or paper)
- See current positions and unrealized P&L
- Get an overall portfolio summary

## Live Account (Requires KMS Auth)

These commands require `--kms-key-id` or `POLYMARKET_KMS_KEY_ID` environment variable.

### Balance

```bash
# USDC balance and per-contract allowances
polymarket-trader account balance --kms-key-id <key-id> --json
```

Shows available USDC and any approved token allowances for the Polymarket contracts.

### Trade History

```bash
# Recent trades (default 25)
polymarket-trader account trades --kms-key-id <key-id> --json

# More history
polymarket-trader account trades --kms-key-id <key-id> --limit 100 --json
```

Returns filled trades with: market, side, price, size, timestamp.

## Paper Trading Portfolio

No authentication needed — all state is local.

### Positions

```bash
# Net positions aggregated from all trades
polymarket-trader dry-run positions --json
```

Output per position:
- `token_id` — the outcome token
- `slug`, `question`, `outcome` — market metadata (if available)
- `side` — "long" or "short"
- `net_size` — current share count
- `avg_price` — weighted average entry price
- `total_cost` — total USDC spent on this position

### P&L Report

```bash
# Unrealized P&L against live prices
polymarket-trader dry-run pnl --json
```

Output includes per-position:
- `current_price` — live midpoint from Polymarket
- `unrealized_pnl` — gain/loss if closed now
- `value` — current market value of the position

And totals:
- `starting_balance` — initial capital
- `current_balance` — available cash
- `total_position_value` — sum of all position values
- `total_value` — cash + position value
- `total_pnl` — overall gain/loss vs starting balance

### Full Portfolio

```bash
# Everything in one view
polymarket-trader dry-run portfolio --json
```

The most complete view — combines positions, cash, totals, and P&L in a single response.

### Trade History

```bash
polymarket-trader dry-run trades --json
polymarket-trader dry-run trades --limit 100 --json
```

## Workflow: Portfolio Health Check

1. **Check cash position**:
   ```bash
   polymarket-trader dry-run portfolio --json
   ```

2. **Review individual positions** and their P&L:
   ```bash
   polymarket-trader dry-run pnl --json
   ```

3. **Investigate underperformers** — check current market state:
   ```bash
   polymarket-trader prices midpoint <slug> --json
   polymarket-trader prices spread <slug> --json
   ```

4. **Decide** whether to hold, add, or close positions based on findings.

## Notes

- Live account commands require AWS KMS authentication (see trade-execution skill)
- Paper trading portfolio is independent from live — they do not share state
- P&L calculations use live midpoint prices; if a market is illiquid, the price may be stale
- Always use `--json` flag when parsing output programmatically
