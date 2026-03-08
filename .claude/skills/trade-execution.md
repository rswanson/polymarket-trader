# Trade Execution

Place real orders on Polymarket with live funds.

## When to Use

Use this skill when the user **explicitly** wants to:
- Buy or sell positions on Polymarket with real USDC
- Place limit or market orders on the live order book

**Important:** If the user's intent is unclear, default to the paper-trading skill instead. Only use this skill when the user has clearly indicated they want to trade with real money.

## Prerequisites

- **AWS KMS key** configured with a Polymarket-registered wallet
- Pass via `--kms-key-id <key-id>` or set `POLYMARKET_KMS_KEY_ID` environment variable
- AWS credentials available (env vars, `~/.aws/credentials`, or IAM role)
- USDC balance on Polymarket (check with `account balance`)

## Commands

### Limit Orders

Place an order at a specific price. It rests on the book until filled or cancelled.

```bash
# Buy 100 shares of "Yes" at $0.65 each
polymarket-trader orders limit <market> buy 0.65 100 --kms-key-id <key-id> --json

# Sell 50 shares at $0.70 each
polymarket-trader orders limit <market> sell 0.70 50 --kms-key-id <key-id> --json

# Target a specific outcome in multi-outcome markets
polymarket-trader orders limit president-2028 buy 0.30 200 --outcome "Harris" --kms-key-id <key-id> --json
```

Parameters:
- `<market>` — slug (e.g., `inflation-2026`) or token ID
- `<side>` — `buy` or `sell`
- `<price>` — limit price between 0.01 and 0.99
- `<size>` — number of shares

### Market Orders

Buy or sell at the best available price immediately. Specify USDC amount, not shares.

```bash
# Buy $100 worth of shares at best available price
polymarket-trader orders market <market> buy 100 --kms-key-id <key-id> --json

# Sell $50 worth
polymarket-trader orders market <market> sell 50 --kms-key-id <key-id> --json
```

Parameters:
- `<market>` — slug or token ID
- `<side>` — `buy` or `sell`
- `<amount>` — USDC amount to spend/receive

## Mandatory Pre-Trade Checklist

**Before placing ANY real order, always:**

1. **Verify the market** — show details and confirm it's the right one:
   ```bash
   polymarket-trader markets show <slug> --json
   ```

2. **Check current price** — ensure the order price makes sense:
   ```bash
   polymarket-trader prices midpoint <slug> --json
   polymarket-trader prices spread <slug> --json
   ```

3. **Check balance** — ensure sufficient funds:
   ```bash
   polymarket-trader account balance --kms-key-id <key-id> --json
   ```

4. **Confirm with the user** — present a clear summary:
   ```
   Order summary:
   - Market: [question]
   - Outcome: [outcome name]
   - Side: [buy/sell]
   - Price: $[price]
   - Size: [shares] shares
   - Total cost: $[price × size]

   Proceed? (yes/no)
   ```

5. **Only then execute** the order after explicit user approval.

## Workflow: Placing a Trade

1. **Research** the market:
   ```bash
   polymarket-trader markets show <slug> --json
   polymarket-trader prices midpoint <slug> --json
   polymarket-trader prices book <slug> --json
   ```

2. **Check balance**:
   ```bash
   polymarket-trader account balance --kms-key-id <key-id> --json
   ```

3. **Present order summary** to user and get confirmation.

4. **Execute**:
   ```bash
   polymarket-trader orders limit <slug> buy <price> <size> --kms-key-id <key-id> --json
   ```

5. **Verify** the order was placed:
   ```bash
   polymarket-trader orders list --kms-key-id <key-id> --json
   ```

## Guardrails

- **NEVER place an order without explicit user confirmation** — always present the order summary first
- **NEVER auto-trade** based on signals or strategies without user approval for each order
- **Check balance** before every trade to avoid insufficient funds errors
- **Verify the market** — confirm the slug resolves to the intended market
- **For multi-outcome markets**, always specify `--outcome` to avoid trading the wrong outcome
- **Prefer limit orders** over market orders for better price control
- If the user asks to "test" a trade, use the paper-trading skill instead

## Error Handling

- **"Insufficient balance"** — not enough USDC; show current balance and the shortfall
- **"Authentication failed"** — check KMS key ID and AWS credentials are correct
- **"Market not found"** — verify the slug with `markets show`; it may be closed or resolved
- **"Invalid price"** — price must be between 0.01 and 0.99
- **"Order rejected"** — the CLOB may reject orders that cross the book at invalid prices; check the order book

## Notes

- Orders are signed with AWS KMS (EIP-712) — no private keys are exposed
- Limit orders may not fill immediately; monitor with `orders list`
- Market orders fill at best available price, which may differ from the midpoint
- Polymarket uses USDC on Polygon — gas fees are negligible but non-zero
- Always use `--json` flag when parsing output programmatically
