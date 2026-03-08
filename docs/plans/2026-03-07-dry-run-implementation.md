# Dry-Run Mode Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a `dry-run` subcommand group that simulates trades at real midpoint prices, tracks a virtual portfolio in SQLite, and reports P&L.

**Architecture:** New `dry_run` module handles SQLite persistence and portfolio math. New `commands/dry_run.rs` implements CLI handlers. Simulated orders fetch real midpoints from the unauthenticated CLOB client and record instant fills. No KMS auth needed.

**Tech Stack:** rusqlite (bundled SQLite), uuid v4 for trade IDs, chrono for timestamps, existing polymarket-client-sdk for midpoint fetches

---

### Task 1: Add Dependencies

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add new dependencies to Cargo.toml**

Add these lines to `[dependencies]`:

```toml
rusqlite = { version = "0.34", features = ["bundled"] }
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
dirs = "6"
```

**Step 2: Verify it compiles**

Run: `cargo check`
Expected: Compiles (new deps downloaded)

**Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "feat(dry-run): add rusqlite, uuid, chrono, dirs dependencies"
```

---

### Task 2: SQLite Database Module

**Files:**
- Create: `src/dry_run/mod.rs`
- Create: `src/dry_run/db.rs`

**Step 1: Create the dry_run module root**

Create `src/dry_run/mod.rs`:

```rust
pub mod db;
pub mod portfolio;
```

**Step 2: Create the database module**

Create `src/dry_run/db.rs`:

```rust
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const DEFAULT_STARTING_BALANCE: &str = "1000.00";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: String,
    pub token_id: String,
    pub side: String,
    pub price: String,
    pub size: String,
    pub cost: String,
    pub timestamp: String,
}

pub struct DryRunDb {
    conn: Connection,
}

impl DryRunDb {
    pub fn open() -> Result<Self> {
        let path = db_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create ~/.polymarket directory")?;
        }
        let conn = Connection::open(&path)
            .context("Failed to open dry-run database")?;
        let db = Self { conn };
        db.ensure_schema()?;
        Ok(db)
    }

    fn ensure_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS trades (
                id TEXT PRIMARY KEY,
                token_id TEXT NOT NULL,
                side TEXT NOT NULL,
                price TEXT NOT NULL,
                size TEXT NOT NULL,
                cost TEXT NOT NULL,
                timestamp TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            INSERT OR IGNORE INTO state (key, value) VALUES ('starting_balance', ?1);
            INSERT OR IGNORE INTO state (key, value) VALUES ('balance', ?1);",
        ).ok();
        // execute_batch doesn't support params, so use separate statements for initial state
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM state WHERE key = 'balance'",
            [],
            |row| row.get(0),
        )?;
        if count == 0 {
            self.conn.execute(
                "INSERT INTO state (key, value) VALUES ('starting_balance', ?1)",
                [DEFAULT_STARTING_BALANCE],
            )?;
            self.conn.execute(
                "INSERT INTO state (key, value) VALUES ('balance', ?1)",
                [DEFAULT_STARTING_BALANCE],
            )?;
        }
        Ok(())
    }

    pub fn get_balance(&self) -> Result<String> {
        let balance: String = self.conn.query_row(
            "SELECT value FROM state WHERE key = 'balance'",
            [],
            |row| row.get(0),
        )?;
        Ok(balance)
    }

    pub fn get_starting_balance(&self) -> Result<String> {
        let balance: String = self.conn.query_row(
            "SELECT value FROM state WHERE key = 'starting_balance'",
            [],
            |row| row.get(0),
        )?;
        Ok(balance)
    }

    pub fn update_balance(&self, new_balance: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE state SET value = ?1 WHERE key = 'balance'",
            [new_balance],
        )?;
        Ok(())
    }

    pub fn insert_trade(&self, trade: &Trade) -> Result<()> {
        self.conn.execute(
            "INSERT INTO trades (id, token_id, side, price, size, cost, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            [
                &trade.id,
                &trade.token_id,
                &trade.side,
                &trade.price,
                &trade.size,
                &trade.cost,
                &trade.timestamp,
            ],
        )?;
        Ok(())
    }

    pub fn delete_trade(&self, trade_id: &str) -> Result<Option<Trade>> {
        let trade = self.get_trade(trade_id)?;
        if trade.is_some() {
            self.conn.execute("DELETE FROM trades WHERE id = ?1", [trade_id])?;
        }
        Ok(trade)
    }

    pub fn get_trade(&self, trade_id: &str) -> Result<Option<Trade>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, token_id, side, price, size, cost, timestamp FROM trades WHERE id = ?1",
        )?;
        let trade = stmt.query_row([trade_id], |row| {
            Ok(Trade {
                id: row.get(0)?,
                token_id: row.get(1)?,
                side: row.get(2)?,
                price: row.get(3)?,
                size: row.get(4)?,
                cost: row.get(5)?,
                timestamp: row.get(6)?,
            })
        }).optional()?;
        Ok(trade)
    }

    pub fn list_trades(&self, limit: usize) -> Result<Vec<Trade>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, token_id, side, price, size, cost, timestamp
             FROM trades ORDER BY timestamp DESC LIMIT ?1",
        )?;
        let trades = stmt.query_map([limit], |row| {
            Ok(Trade {
                id: row.get(0)?,
                token_id: row.get(1)?,
                side: row.get(2)?,
                price: row.get(3)?,
                size: row.get(4)?,
                cost: row.get(5)?,
                timestamp: row.get(6)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(trades)
    }

    pub fn all_trades(&self) -> Result<Vec<Trade>> {
        self.list_trades(usize::MAX)
    }

    pub fn reset(&self, starting_balance: &str) -> Result<()> {
        self.conn.execute_batch("DELETE FROM trades; DELETE FROM state;")?;
        self.conn.execute(
            "INSERT INTO state (key, value) VALUES ('starting_balance', ?1)",
            [starting_balance],
        )?;
        self.conn.execute(
            "INSERT INTO state (key, value) VALUES ('balance', ?1)",
            [starting_balance],
        )?;
        Ok(())
    }
}

fn db_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(".polymarket").join("dry-run.db"))
}
```

Note: You'll need to add `use rusqlite::OptionalExtension;` for the `.optional()` call on `get_trade`.

**Step 3: Add `mod dry_run;` to main.rs**

Add `mod dry_run;` after the existing module declarations in `src/main.rs`.

**Step 4: Verify it compiles**

Run: `cargo check`
Expected: Compiles (with dead code warnings, that's fine)

**Step 5: Commit**

```bash
git add src/dry_run/mod.rs src/dry_run/db.rs src/main.rs
git commit -m "feat(dry-run): add SQLite database module"
```

---

### Task 3: Portfolio Module

**Files:**
- Create: `src/dry_run/portfolio.rs`

**Step 1: Create the portfolio module**

Create `src/dry_run/portfolio.rs`:

```rust
use anyhow::Result;
use rust_decimal::Decimal;
use serde::Serialize;
use std::collections::HashMap;
use std::str::FromStr;

use super::db::{DryRunDb, Trade};

#[derive(Debug, Clone, Serialize)]
pub struct Position {
    pub token_id: String,
    pub net_size: String,
    pub side: String,
    pub avg_price: String,
    pub total_cost: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PnlReport {
    pub starting_balance: String,
    pub current_balance: String,
    pub positions: Vec<PositionPnl>,
    pub total_unrealized_pnl: String,
    pub total_pnl: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PositionPnl {
    pub token_id: String,
    pub side: String,
    pub size: String,
    pub avg_price: String,
    pub current_price: String,
    pub unrealized_pnl: String,
}

pub fn compute_positions(trades: &[Trade]) -> Result<Vec<Position>> {
    // Aggregate by token_id: buys add to position, sells subtract
    let mut positions: HashMap<String, (Decimal, Decimal)> = HashMap::new(); // (net_size, total_cost)

    for trade in trades {
        let size = Decimal::from_str(&trade.size)?;
        let price = Decimal::from_str(&trade.price)?;
        let entry = positions.entry(trade.token_id.clone()).or_default();

        match trade.side.as_str() {
            "buy" => {
                entry.0 += size;
                entry.1 += price * size;
            }
            "sell" => {
                entry.0 -= size;
                entry.1 -= price * size;
            }
            _ => {}
        }
    }

    let mut result: Vec<Position> = positions
        .into_iter()
        .filter(|(_, (size, _))| !size.is_zero())
        .map(|(token_id, (net_size, total_cost))| {
            let avg_price = if !net_size.is_zero() {
                (total_cost / net_size).abs()
            } else {
                Decimal::ZERO
            };
            let side = if net_size > Decimal::ZERO { "long" } else { "short" };
            Position {
                token_id,
                net_size: net_size.abs().to_string(),
                side: side.to_string(),
                avg_price: avg_price.round_dp(6).to_string(),
                total_cost: total_cost.abs().round_dp(2).to_string(),
            }
        })
        .collect();

    result.sort_by(|a, b| a.token_id.cmp(&b.token_id));
    Ok(result)
}

pub fn compute_pnl(
    positions: &[Position],
    current_prices: &HashMap<String, Decimal>,
    starting_balance: &str,
    current_balance: &str,
) -> Result<PnlReport> {
    let starting = Decimal::from_str(starting_balance)?;
    let current = Decimal::from_str(current_balance)?;

    let mut total_unrealized = Decimal::ZERO;
    let mut position_pnls = Vec::new();

    for pos in positions {
        let size = Decimal::from_str(&pos.net_size)?;
        let avg = Decimal::from_str(&pos.avg_price)?;
        let current_price = current_prices
            .get(&pos.token_id)
            .copied()
            .unwrap_or(avg);

        let unrealized = match pos.side.as_str() {
            "long" => (current_price - avg) * size,
            "short" => (avg - current_price) * size,
            _ => Decimal::ZERO,
        };

        total_unrealized += unrealized;

        position_pnls.push(PositionPnl {
            token_id: pos.token_id.clone(),
            side: pos.side.clone(),
            size: pos.net_size.clone(),
            avg_price: pos.avg_price.clone(),
            current_price: current_price.round_dp(6).to_string(),
            unrealized_pnl: unrealized.round_dp(2).to_string(),
        });
    }

    let total_pnl = (current - starting) + total_unrealized;

    Ok(PnlReport {
        starting_balance: starting_balance.to_string(),
        current_balance: current_balance.to_string(),
        positions: position_pnls,
        total_unrealized_pnl: total_unrealized.round_dp(2).to_string(),
        total_pnl: total_pnl.round_dp(2).to_string(),
    })
}
```

**Step 2: Add `rust_decimal` to Cargo.toml**

The portfolio module uses `rust_decimal::Decimal` for math. Add:

```toml
rust_decimal = "1.40"
```

**Step 3: Verify it compiles**

Run: `cargo check`
Expected: Compiles

**Step 4: Commit**

```bash
git add src/dry_run/portfolio.rs Cargo.toml Cargo.lock
git commit -m "feat(dry-run): add portfolio position and P&L calculations"
```

---

### Task 4: Dry-Run CLI Args

**Files:**
- Modify: `src/cli.rs`

**Step 1: Add DryRun variant to Command enum and DryRunCommand**

Add to `src/cli.rs` — add `DryRun` to the `Command` enum and the supporting types:

After the existing `Account(AccountArgs)` in the `Command` enum, add:

```rust
    /// Simulated trading (paper trading) commands
    DryRun(DryRunArgs),
```

Then add these new types at the bottom of `cli.rs`:

```rust
#[derive(Parser)]
pub struct DryRunArgs {
    #[command(subcommand)]
    pub command: DryRunCommand,
}

#[derive(Subcommand)]
pub enum DryRunCommand {
    /// Simulate a limit order (fills at current midpoint)
    Limit {
        /// Token ID
        token_id: String,
        /// Side: "buy" or "sell"
        side: String,
        /// Price (for reference, fill is at midpoint)
        price: String,
        /// Size in shares
        size: String,
    },
    /// Simulate a market order (fills at current midpoint)
    Market {
        /// Token ID
        token_id: String,
        /// Side: "buy" or "sell"
        side: String,
        /// Amount in USDC
        amount: String,
    },
    /// Remove a simulated trade
    Cancel {
        /// Trade ID to cancel
        trade_id: String,
    },
    /// Show current simulated positions
    Positions,
    /// Show simulated trade history
    Trades {
        /// Maximum number of trades to show
        #[arg(long, default_value = "25")]
        limit: usize,
    },
    /// Show profit and loss report
    Pnl,
    /// Reset dry-run state
    Reset {
        /// Starting balance in USDC
        #[arg(long, default_value = "1000.00")]
        balance: String,
    },
}
```

**Step 2: Verify it compiles**

Run: `cargo check`
Expected: Compiles (warning about unused DryRunCommand, that's fine)

**Step 3: Commit**

```bash
git add src/cli.rs
git commit -m "feat(dry-run): add CLI argument definitions"
```

---

### Task 5: Dry-Run Command Handlers

**Files:**
- Create: `src/commands/dry_run.rs`
- Modify: `src/commands/mod.rs`

**Step 1: Create the dry-run command handlers**

Create `src/commands/dry_run.rs`:

```rust
use std::collections::HashMap;
use std::str::FromStr;

use anyhow::{Context, Result};
use chrono::Utc;
use polymarket_client_sdk::auth::state::State;
use polymarket_client_sdk::clob::types::request::MidpointRequest;
use polymarket_client_sdk::clob::Client;
use polymarket_client_sdk::types::{Decimal, U256};
use serde::Serialize;
use uuid::Uuid;

use crate::dry_run::db::{DryRunDb, Trade};
use crate::dry_run::portfolio;
use crate::output::print_output;

fn parse_side(s: &str) -> Result<String> {
    match s.to_lowercase().as_str() {
        "buy" => Ok("buy".to_string()),
        "sell" => Ok("sell".to_string()),
        other => anyhow::bail!("Invalid side '{other}', expected 'buy' or 'sell'"),
    }
}

async fn fetch_midpoint<S: State>(client: &Client<S>, token_id_str: &str) -> Result<Decimal> {
    let token_id = U256::from_str(token_id_str)
        .map_err(|e| anyhow::anyhow!("Invalid token ID: {e}"))?;
    let request = MidpointRequest::builder().token_id(token_id).build();
    let response = client.midpoint(&request).await
        .context("Failed to fetch midpoint price")?;
    Ok(response.mid)
}

#[derive(Serialize)]
struct SimulatedTradeResult {
    id: String,
    token_id: String,
    side: String,
    fill_price: String,
    size: String,
    cost: String,
    balance_after: String,
}

pub async fn place_limit<S: State>(
    client: &Client<S>,
    token_id: &str,
    side_str: &str,
    _price_str: &str,
    size_str: &str,
    json: bool,
) -> Result<()> {
    let side = parse_side(side_str)?;
    let size = Decimal::from_str(size_str)
        .map_err(|e| anyhow::anyhow!("Invalid size: {e}"))?;
    let midpoint = fetch_midpoint(client, token_id).await?;
    let cost = midpoint * size;

    let db = DryRunDb::open()?;
    let mut balance = Decimal::from_str(&db.get_balance()?)?;

    if side == "buy" {
        anyhow::ensure!(balance >= cost, "Insufficient balance: have {balance}, need {cost}");
        balance -= cost;
    } else {
        balance += cost;
    }

    let trade = Trade {
        id: Uuid::new_v4().to_string(),
        token_id: token_id.to_string(),
        side: side.clone(),
        price: midpoint.round_dp(6).to_string(),
        size: size.to_string(),
        cost: cost.round_dp(2).to_string(),
        timestamp: Utc::now().to_rfc3339(),
    };

    db.insert_trade(&trade)?;
    db.update_balance(&balance.round_dp(2).to_string())?;

    let result = SimulatedTradeResult {
        id: trade.id,
        token_id: token_id.to_string(),
        side,
        fill_price: midpoint.round_dp(6).to_string(),
        size: size.to_string(),
        cost: cost.round_dp(2).to_string(),
        balance_after: balance.round_dp(2).to_string(),
    };

    let headers = &["ID", "Token", "Side", "Fill Price", "Size", "Cost", "Balance"];
    let rows = vec![vec![
        result.id.clone(),
        result.token_id.chars().take(12).collect::<String>() + "...",
        result.side.clone(),
        result.fill_price.clone(),
        result.size.clone(),
        result.cost.clone(),
        result.balance_after.clone(),
    ]];
    print_output(json, headers, rows, &result);

    Ok(())
}

pub async fn place_market<S: State>(
    client: &Client<S>,
    token_id: &str,
    side_str: &str,
    amount_str: &str,
    json: bool,
) -> Result<()> {
    let side = parse_side(side_str)?;
    let amount = Decimal::from_str(amount_str)
        .map_err(|e| anyhow::anyhow!("Invalid amount: {e}"))?;
    let midpoint = fetch_midpoint(client, token_id).await?;

    anyhow::ensure!(!midpoint.is_zero(), "Midpoint is zero, cannot calculate size");
    let size = (amount / midpoint).round_dp(6);
    let cost = amount;

    let db = DryRunDb::open()?;
    let mut balance = Decimal::from_str(&db.get_balance()?)?;

    if side == "buy" {
        anyhow::ensure!(balance >= cost, "Insufficient balance: have {balance}, need {cost}");
        balance -= cost;
    } else {
        balance += cost;
    }

    let trade = Trade {
        id: Uuid::new_v4().to_string(),
        token_id: token_id.to_string(),
        side: side.clone(),
        price: midpoint.round_dp(6).to_string(),
        size: size.to_string(),
        cost: cost.round_dp(2).to_string(),
        timestamp: Utc::now().to_rfc3339(),
    };

    db.insert_trade(&trade)?;
    db.update_balance(&balance.round_dp(2).to_string())?;

    let result = SimulatedTradeResult {
        id: trade.id,
        token_id: token_id.to_string(),
        side,
        fill_price: midpoint.round_dp(6).to_string(),
        size: size.to_string(),
        cost: cost.round_dp(2).to_string(),
        balance_after: balance.round_dp(2).to_string(),
    };

    let headers = &["ID", "Token", "Side", "Fill Price", "Size", "Cost", "Balance"];
    let rows = vec![vec![
        result.id.clone(),
        result.token_id.chars().take(12).collect::<String>() + "...",
        result.side.clone(),
        result.fill_price.clone(),
        result.size.clone(),
        result.cost.clone(),
        result.balance_after.clone(),
    ]];
    print_output(json, headers, rows, &result);

    Ok(())
}

pub fn cancel(trade_id: &str, json: bool) -> Result<()> {
    let db = DryRunDb::open()?;
    let trade = db.delete_trade(trade_id)?
        .ok_or_else(|| anyhow::anyhow!("Trade '{trade_id}' not found"))?;

    // Reverse the balance effect
    let cost = Decimal::from_str(&trade.cost)?;
    let mut balance = Decimal::from_str(&db.get_balance()?)?;
    if trade.side == "buy" {
        balance += cost;
    } else {
        balance -= cost;
    }
    db.update_balance(&balance.round_dp(2).to_string())?;

    #[derive(Serialize)]
    struct CancelResult { canceled_id: String, balance_after: String }
    let result = CancelResult {
        canceled_id: trade_id.to_string(),
        balance_after: balance.round_dp(2).to_string(),
    };

    let headers = &["Canceled ID", "Balance After"];
    let rows = vec![vec![result.canceled_id.clone(), result.balance_after.clone()]];
    print_output(json, headers, rows, &result);

    Ok(())
}

pub fn positions(json: bool) -> Result<()> {
    let db = DryRunDb::open()?;
    let trades = db.all_trades()?;
    let positions = portfolio::compute_positions(&trades)?;

    let headers = &["Token ID", "Side", "Size", "Avg Price", "Total Cost"];
    let rows: Vec<Vec<String>> = positions
        .iter()
        .map(|p| vec![
            p.token_id.chars().take(16).collect::<String>() + "...",
            p.side.clone(),
            p.net_size.clone(),
            p.avg_price.clone(),
            p.total_cost.clone(),
        ])
        .collect();
    print_output(json, headers, rows, &positions);

    Ok(())
}

pub fn trades(limit: usize, json: bool) -> Result<()> {
    let db = DryRunDb::open()?;
    let trades = db.list_trades(limit)?;

    let headers = &["ID", "Token", "Side", "Price", "Size", "Cost", "Time"];
    let rows: Vec<Vec<String>> = trades
        .iter()
        .map(|t| vec![
            t.id.chars().take(8).collect::<String>() + "...",
            t.token_id.chars().take(12).collect::<String>() + "...",
            t.side.clone(),
            t.price.clone(),
            t.size.clone(),
            t.cost.clone(),
            t.timestamp.clone(),
        ])
        .collect();
    print_output(json, headers, rows, &trades);

    Ok(())
}

pub async fn pnl<S: State>(client: &Client<S>, json: bool) -> Result<()> {
    let db = DryRunDb::open()?;
    let all_trades = db.all_trades()?;
    let positions = portfolio::compute_positions(&all_trades)?;

    // Fetch current midpoints for all positions
    let mut current_prices: HashMap<String, Decimal> = HashMap::new();
    for pos in &positions {
        let midpoint = fetch_midpoint(client, &pos.token_id).await?;
        current_prices.insert(pos.token_id.clone(), midpoint);
    }

    let report = portfolio::compute_pnl(
        &positions,
        &current_prices,
        &db.get_starting_balance()?,
        &db.get_balance()?,
    )?;

    if json {
        print_output(true, &[], vec![], &report);
    } else {
        println!("Starting Balance: {} USDC", report.starting_balance);
        println!("Current Balance:  {} USDC", report.current_balance);
        println!();

        if !report.positions.is_empty() {
            let headers = &["Token", "Side", "Size", "Avg Price", "Current", "Unrealized P&L"];
            let rows: Vec<Vec<String>> = report.positions.iter().map(|p| vec![
                p.token_id.chars().take(12).collect::<String>() + "...",
                p.side.clone(),
                p.size.clone(),
                p.avg_price.clone(),
                p.current_price.clone(),
                p.unrealized_pnl.clone(),
            ]).collect();
            print_output(false, headers, rows, &report.positions);
            println!();
        }

        println!("Unrealized P&L: {} USDC", report.total_unrealized_pnl);
        println!("Total P&L:      {} USDC", report.total_pnl);
    }

    Ok(())
}

pub fn reset(balance: &str, json: bool) -> Result<()> {
    // Validate balance is a valid decimal
    Decimal::from_str(balance)
        .map_err(|e| anyhow::anyhow!("Invalid balance: {e}"))?;

    let db = DryRunDb::open()?;
    db.reset(balance)?;

    #[derive(Serialize)]
    struct ResetResult { starting_balance: String }
    let result = ResetResult { starting_balance: balance.to_string() };

    let headers = &["Starting Balance"];
    let rows = vec![vec![balance.to_string()]];
    print_output(json, headers, rows, &result);

    Ok(())
}
```

**Step 2: Add `dry_run` to commands/mod.rs**

Add to `src/commands/mod.rs`:

```rust
pub mod dry_run;
```

**Step 3: Verify it compiles**

Run: `cargo check`
Expected: Compiles

**Step 4: Commit**

```bash
git add src/commands/dry_run.rs src/commands/mod.rs
git commit -m "feat(dry-run): add command handlers for all dry-run subcommands"
```

---

### Task 6: Wire Dry-Run into Main Dispatch

**Files:**
- Modify: `src/main.rs`

**Step 1: Add DryRunCommand to imports and dispatch**

In `src/main.rs`, update the import line to include `DryRunCommand`:

```rust
use cli::{AccountCommand, Cli, Command, DryRunCommand, MarketsCommand, OrdersCommand, PricesCommand};
```

Add a new arm to the `match &cli.command` block, after the `Command::Account` arm:

```rust
        Command::DryRun(args) => {
            let client = client::create_unauthenticated_client(&cli.clob_host)?;
            match &args.command {
                DryRunCommand::Limit { token_id, side, price, size } => {
                    commands::dry_run::place_limit(&client, token_id, side, price, size, json).await?;
                }
                DryRunCommand::Market { token_id, side, amount } => {
                    commands::dry_run::place_market(&client, token_id, side, amount, json).await?;
                }
                DryRunCommand::Cancel { trade_id } => {
                    commands::dry_run::cancel(trade_id, json)?;
                }
                DryRunCommand::Positions => {
                    commands::dry_run::positions(json)?;
                }
                DryRunCommand::Trades { limit } => {
                    commands::dry_run::trades(*limit, json)?;
                }
                DryRunCommand::Pnl => {
                    commands::dry_run::pnl(&client, json).await?;
                }
                DryRunCommand::Reset { balance } => {
                    commands::dry_run::reset(balance, json)?;
                }
            }
        }
```

**Step 2: Verify it compiles**

Run: `cargo check`
Expected: Compiles with no errors

**Step 3: Verify CLI help**

Run: `cargo run -- dry-run --help`
Expected: Shows all dry-run subcommands

**Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat(dry-run): wire dry-run commands into main dispatch"
```

---

### Task 7: Build Verification and Cleanup

**Files:**
- All files

**Step 1: Run clippy**

Run: `cargo clippy -- -D warnings`
Fix any warnings.

**Step 2: Run cargo fmt**

Run: `cargo fmt`

**Step 3: Verify release build**

Run: `cargo build --release`
Expected: Compiles successfully

**Step 4: Test help output for all dry-run commands**

Run: `cargo run --release -- dry-run --help`
Run: `cargo run --release -- dry-run limit --help`
Run: `cargo run --release -- dry-run pnl --help`

**Step 5: Commit any cleanup**

```bash
git add -A
git commit -m "chore: clippy fixes and formatting for dry-run"
```

---

### Task 8: Update README and CLAUDE.md

**Files:**
- Modify: `README.md`
- Modify: `CLAUDE.md`

**Step 1: Add dry-run section to README.md**

Add a new section after "### Account" in README.md:

```markdown
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
```

**Step 2: Update CLAUDE.md**

Add to the module layout section in CLAUDE.md:

```markdown
- `dry_run/db.rs` — SQLite persistence for simulated trades and balance
- `dry_run/portfolio.rs` — Position aggregation and P&L math
- `commands/dry_run.rs` — Dry-run subcommand handlers (uses unauthenticated client)
```

Add `rusqlite` to the key dependencies note.

**Step 3: Commit**

```bash
git add README.md CLAUDE.md
git commit -m "docs: add dry-run mode documentation"
```
