# User Feedback Round 2 — Design

Date: 2026-03-08

## Scope

Four features based on active user feedback. Ordered by priority.

### 1. Sell by Position Index

**Problem:** User must know the exact slug to close a position. Typos cause 404s.

**Solution:** `dry-run close` accepts a 1-based position index matching `dry-run portfolio` output order.

- `dry-run close 3` → resolves to the 3rd position in portfolio order
- `dry-run close <slug>` → existing behavior unchanged
- Detection: if arg parses as a positive integer within the position count, treat as index. Otherwise treat as slug/token ID.
- Position ordering must be deterministic and consistent between `portfolio` and `close` (currently sorted by token_id).

**Changes:** `cli.rs` (arg parsing), `commands/dry_run.rs` (close command), `dry_run/portfolio.rs` (expose indexed position lookup).

### 2. Better `--outcome` Discoverability

**Problem:** `--outcome no` flag is not discoverable. User almost placed a wrong trade.

**Solution:** No new syntax. Two changes:
- After a trade on a binary market without `--outcome`, print: `Note: defaulted to YES outcome. Use --outcome no for the NO side.`
- Improve `--outcome` Clap help text with examples.

**Changes:** `commands/dry_run.rs` (post-trade message), `cli.rs` (help text).

### 3. Threshold Alerts

**Problem:** User manually checks prices every 30 minutes for take-profit/stop-loss levels.

**Solution A — `dry-run alerts` command (polling):**
- Flags: `--take-profit <pct>` (default 15), `--stop-loss <pct>` (default 20), `--interval <secs>` (default 60)
- Loads open positions, polls midpoints in a loop
- Prints colored alerts when positions approach (within 80% of threshold) or breach TP/SL levels
- Ctrl+C to exit (same pattern as `markets watch`)

**Solution B — Threshold indicators in `dry-run portfolio` (one-shot):**
- Optional `--take-profit` and `--stop-loss` flags on `portfolio`
- Adds status column showing threshold proximity for each position
- No polling, no new command — just extra info on existing output

Both solutions implemented.

**Changes:** `cli.rs` (new alerts subcommand, portfolio flags), `commands/dry_run.rs` (alerts loop, portfolio enhancement), reuse existing midpoint fetching.

### 4. Realized P&L Tracking

**Problem:** Selling a position loses its P&L from the portfolio view. No way to see overall performance.

**Solution:**
- `compute_positions()` in `portfolio.rs` currently discards closed positions. Instead, compute realized P&L before discarding: realized_pnl = (sell_price - avg_buy_price) x sell_size.
- Track total realized P&L, closed trade count, wins vs losses.
- Add realized P&L line to `dry-run portfolio` output.
- New `dry-run summary` command showing: realized P&L, unrealized P&L, net P&L, win rate, starting balance, current balance, total portfolio value.

No schema changes — all data is already in the `trades` table.

**Changes:** `cli.rs` (summary subcommand), `dry_run/portfolio.rs` (realized P&L math), `commands/dry_run.rs` (summary command, portfolio enhancement).

## Deferred

- **Market category/volume filters** — existing query/sort is sufficient; improve docs instead.
- **Price history/sparkline** — requires new data source or price-recording system. Larger project.
- **Webhook/script alerts** — users can wrap `alerts` in shell scripts for now.

## Non-Goals

- No changes to authenticated (live trading) commands
- No new external dependencies
- No database schema migrations
