# Market Research

Search, browse, and analyze Polymarket prediction markets.

## When to Use

Use this skill when the user wants to:
- Find markets on a topic (politics, crypto, sports, etc.)
- Check current odds/probabilities for an event
- View price spreads or order book depth
- See what's trending on Polymarket
- Monitor live price movements

## Commands

### Search & Browse

```bash
# Search markets by keyword
polymarket-trader markets list --query "election" --json

# Limit results
polymarket-trader markets list --query "bitcoin" --limit 5 --json

# Filter by minimum volume
polymarket-trader markets list --query "fed" --volume 100000 --json

# Sort by volume (default) or other criteria
polymarket-trader markets list --query "ai" --sort volume --json

# Include closed/resolved markets
polymarket-trader markets list --query "superbowl" --closed --json

# Top 10 trending markets by 24h volume
polymarket-trader markets trending --json
```

### Market Details

```bash
# Show full market details by slug
polymarket-trader markets show inflation-2026 --json

# Show by condition ID (raw token)
polymarket-trader markets show "12345678901234567890" --json
```

The JSON output includes:
- `question` — the market question
- `outcomes` — list of outcome names
- `tokens` — token IDs for each outcome (needed for trading)
- `volume`, `liquidity`, `end_date` — market stats

### Prices

```bash
# Current midpoint price (probability) for an outcome
polymarket-trader prices midpoint inflation-2026 --json

# For a specific outcome in a multi-outcome market
polymarket-trader prices midpoint president-2028 --outcome "Harris" --json

# Bid-ask spread
polymarket-trader prices spread inflation-2026 --json

# Full order book (all bids and asks)
polymarket-trader prices book inflation-2026 --json
```

Prices are between 0.00 and 1.00, representing the implied probability. A midpoint of 0.65 means the market implies ~65% probability.

### Live Price Watching

```bash
# Watch one or more markets with live updates (default 5s interval)
polymarket-trader markets watch inflation-2026 --json

# Custom refresh interval
polymarket-trader markets watch btc-100k --interval 10 --json

# Watch multiple markets
polymarket-trader markets watch inflation-2026 btc-100k fed-rate-cut --json
```

Watch mode streams JSON lines, each containing current prices and deltas from session start. Use Ctrl+C to stop.

## Workflow: Researching a Trading Opportunity

1. **Discover** — Search for markets or check trending:
   ```bash
   polymarket-trader markets trending --json
   ```

2. **Investigate** — Get full details on interesting markets:
   ```bash
   polymarket-trader markets show <slug> --json
   ```

3. **Analyze prices** — Check current odds and liquidity:
   ```bash
   polymarket-trader prices midpoint <slug> --json
   polymarket-trader prices spread <slug> --json
   polymarket-trader prices book <slug> --json
   ```

4. **Monitor** — Watch for price movements before deciding:
   ```bash
   polymarket-trader markets watch <slug> --json
   ```

## Interpreting Results

- **Midpoint price** = implied probability (0.65 = 65% chance)
- **Spread** = difference between best bid and ask; tighter spread = more liquid
- **Volume** = total USDC traded; higher = more market confidence
- **Order book depth** = how much size is available at various price levels

## Notes

- All commands here are **unauthenticated** — no KMS key or wallet needed
- Market slugs are the preferred way to reference markets (human-readable)
- Multi-outcome markets require `--outcome` to specify which outcome to price
- For binary markets (Yes/No), the default outcome is the first one (typically "Yes")
- Always use `--json` flag when parsing output programmatically
