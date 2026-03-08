# Order Management

View, track, and cancel orders on Polymarket.

## When to Use

Use this skill when the user wants to:
- See their open or historical orders
- Cancel a specific order
- Cancel all open orders (or all for a specific market)
- Check if an order has been filled

All commands require `--kms-key-id` or `POLYMARKET_KMS_KEY_ID` environment variable.

## Commands

### List Orders

```bash
# Open orders only
polymarket-trader orders list --kms-key-id <key-id> --json

# All orders (including filled, cancelled, expired)
polymarket-trader orders list --all --kms-key-id <key-id> --json
```

Each order in the JSON output includes: order ID, market/token, side, price, size, status, and timestamps.

### Cancel a Specific Order

```bash
polymarket-trader orders cancel <order-id> --kms-key-id <key-id> --json
```

The order ID comes from the `orders list` output or from the response when placing an order.

**Important:** Cancellation is irreversible. Always confirm the order ID with the user before cancelling.

### Cancel All Orders

```bash
# Cancel ALL open orders across all markets
polymarket-trader orders cancel-all --kms-key-id <key-id> --json

# Cancel all open orders for a specific market only
polymarket-trader orders cancel-all --market <condition-id> --kms-key-id <key-id> --json
```

**Important:** `cancel-all` without `--market` cancels EVERY open order. Always confirm with the user before executing this.

## Workflow: Reviewing and Cleaning Up Orders

1. **List open orders** to see what's pending:
   ```bash
   polymarket-trader orders list --kms-key-id <key-id> --json
   ```

2. **Check current prices** to see if orders are likely to fill:
   ```bash
   polymarket-trader prices midpoint <slug> --json
   polymarket-trader prices spread <slug> --json
   ```

3. **Cancel stale orders** that are too far from market:
   ```bash
   polymarket-trader orders cancel <order-id> --kms-key-id <key-id> --json
   ```

4. **Optionally replace** with updated prices (see trade-execution skill).

## Guardrails

- **Always list orders first** before cancelling, so the user can see what will be affected
- **Confirm cancel-all** with the user explicitly — this is a broad destructive action
- **Show order details** (market, side, price, size) when confirming a cancel
- Cancellations are **irreversible** — the order is removed from the book immediately

## Error Handling

- **"Order not found"** — the order may have already been filled or cancelled; check with `orders list --all`
- **"Authentication failed"** — verify KMS key ID and AWS credentials
- **"No open orders"** — nothing to cancel; confirm with `orders list --all` for history

## Notes

- Order management is for **live orders only** — paper trading uses `dry-run cancel`
- The `--market` flag on `cancel-all` takes a condition ID, not a slug
- Always use `--json` flag when parsing output programmatically
