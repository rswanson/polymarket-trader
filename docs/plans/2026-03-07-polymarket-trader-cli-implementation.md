# Polymarket Trader CLI - Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a Rust CLI that authenticates via AWS KMS and interacts with Polymarket's CLOB API for market data and trading.

**Architecture:** Clap-based CLI using `polymarket-client-sdk` for all Polymarket interaction. AWS KMS signing via `alloy-signer-aws`. Type-state authenticated client pattern from the SDK. Output as human-readable tables or JSON.

**Tech Stack:** Rust (edition 2024), polymarket-client-sdk 0.4, alloy 1.6 (signer-aws), aws-config, aws-sdk-kms, clap 4, tokio, comfy-table, tracing

---

### Task 1: Project Scaffolding

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`

**Step 1: Initialize the Cargo project**

Run: `cargo init --name polymarket-trader`

**Step 2: Set up Cargo.toml with all dependencies**

Replace `Cargo.toml` with:

```toml
[package]
name = "polymarket-trader"
version = "0.1.0"
edition = "2024"

[dependencies]
polymarket-client-sdk = { version = "0.4", features = ["clob"] }
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
anyhow = "1"
rust_decimal = "1.40"
rust_decimal_macros = "1.40"
```

**Step 3: Write minimal main.rs that compiles**

```rust
fn main() {
    println!("polymarket-trader");
}
```

**Step 4: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully (may take a while for first build)

**Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs
git commit -m "feat: scaffold project with dependencies"
```

---

### Task 2: CLI Argument Parsing with Clap

**Files:**
- Create: `src/cli.rs`
- Modify: `src/main.rs`

**Step 1: Create the CLI definition**

Create `src/cli.rs`:

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "polymarket-trader", about = "Polymarket trading CLI")]
pub struct Cli {
    /// Output as JSON instead of tables
    #[arg(long, global = true)]
    pub json: bool,

    /// AWS KMS key ID for wallet signing
    #[arg(long, env = "POLYMARKET_KMS_KEY_ID", global = true)]
    pub kms_key_id: String,

    /// Polymarket CLOB API host
    #[arg(long, default_value = "https://clob.polymarket.com", env = "POLYMARKET_CLOB_HOST", global = true)]
    pub clob_host: String,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Market data commands
    Markets(MarketsArgs),
    /// Price and order book commands
    Prices(PricesArgs),
    /// Order management commands
    Orders(OrdersArgs),
    /// Account information commands
    Account(AccountArgs),
}

// -- Markets --

#[derive(Parser)]
pub struct MarketsArgs {
    #[command(subcommand)]
    pub command: MarketsCommand,
}

#[derive(Subcommand)]
pub enum MarketsCommand {
    /// List active markets
    List {
        /// Maximum number of markets to return
        #[arg(long, default_value = "25")]
        limit: usize,
    },
    /// Show market details
    Show {
        /// Condition ID of the market
        condition_id: String,
    },
}

// -- Prices --

#[derive(Parser)]
pub struct PricesArgs {
    #[command(subcommand)]
    pub command: PricesCommand,
}

#[derive(Subcommand)]
pub enum PricesCommand {
    /// Get current midpoint price
    Midpoint {
        /// Token ID
        token_id: String,
    },
    /// Get bid-ask spread
    Spread {
        /// Token ID
        token_id: String,
    },
    /// Get full order book
    Book {
        /// Token ID
        token_id: String,
    },
}

// -- Orders --

#[derive(Parser)]
pub struct OrdersArgs {
    #[command(subcommand)]
    pub command: OrdersCommand,
}

#[derive(Subcommand)]
pub enum OrdersCommand {
    /// List your orders
    List {
        /// Show all orders (including filled/canceled)
        #[arg(long)]
        all: bool,
    },
    /// Place a limit order
    Limit {
        /// Token ID
        token_id: String,
        /// Side: "buy" or "sell"
        side: String,
        /// Price (0.01 - 0.99)
        price: String,
        /// Size in shares
        size: String,
    },
    /// Place a market order
    Market {
        /// Token ID
        token_id: String,
        /// Side: "buy" or "sell"
        side: String,
        /// Amount in USDC to spend
        amount: String,
    },
    /// Cancel an order by ID
    Cancel {
        /// Order ID to cancel
        order_id: String,
    },
    /// Cancel all open orders
    CancelAll {
        /// Only cancel orders for a specific market (condition ID)
        #[arg(long)]
        market: Option<String>,
    },
}

// -- Account --

#[derive(Parser)]
pub struct AccountArgs {
    #[command(subcommand)]
    pub command: AccountCommand,
}

#[derive(Subcommand)]
pub enum AccountCommand {
    /// Show USDC balance and allowance
    Balance,
    /// Show recent trades
    Trades {
        /// Maximum number of trades to return
        #[arg(long, default_value = "25")]
        limit: usize,
    },
}
```

**Step 2: Update main.rs to parse CLI args**

```rust
mod cli;

use clap::Parser;
use cli::Cli;

fn main() {
    let _cli = Cli::parse();
    println!("parsed CLI args successfully");
}
```

**Step 3: Verify it compiles and --help works**

Run: `cargo run -- --help`
Expected: Shows help text with all subcommands

Run: `cargo run -- --kms-key-id test markets list`
Expected: Prints "parsed CLI args successfully"

**Step 4: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "feat: add CLI argument parsing with clap"
```

---

### Task 3: AWS KMS Signer Setup

**Files:**
- Create: `src/signer.rs`
- Modify: `src/main.rs`

**Step 1: Create the signer module**

Create `src/signer.rs`:

```rust
use alloy::signers::aws::AwsSigner;
use aws_config::BehaviorVersion;
use polymarket_client_sdk::POLYGON;

pub async fn create_kms_signer(key_id: &str) -> anyhow::Result<AwsSigner> {
    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let kms_client = aws_sdk_kms::Client::new(&config);

    let signer = AwsSigner::new(kms_client, key_id.to_owned(), Some(POLYGON))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create KMS signer: {e}"))?;

    Ok(signer)
}
```

**Step 2: Wire it into main.rs**

```rust
mod cli;
mod signer;

use clap::Parser;
use cli::Cli;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let _signer = signer::create_kms_signer(&cli.kms_key_id).await?;
    tracing::info!("KMS signer created");

    println!("signer created successfully");
    Ok(())
}
```

**Step 3: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add src/signer.rs src/main.rs
git commit -m "feat: add AWS KMS signer setup"
```

---

### Task 4: Polymarket Client Authentication

**Files:**
- Create: `src/client.rs`
- Modify: `src/main.rs`

**Step 1: Create the client module**

Create `src/client.rs`:

```rust
use alloy::signers::Signer;
use polymarket_client_sdk::auth::state::Authenticated;
use polymarket_client_sdk::auth::Normal;
use polymarket_client_sdk::clob::{Client, Config};

pub async fn create_authenticated_client<S: Signer>(
    host: &str,
    signer: &S,
) -> anyhow::Result<Client<Authenticated<Normal>>> {
    let config = Config::builder().use_server_time(true).build();

    let client = Client::new(host, config)?
        .authentication_builder(signer)
        .authenticate()
        .await?;

    Ok(client)
}
```

**Step 2: Wire into main.rs to test auth flow**

```rust
mod cli;
mod client;
mod signer;

use clap::Parser;
use cli::Cli;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let signer = signer::create_kms_signer(&cli.kms_key_id).await?;
    let _client = client::create_authenticated_client(&cli.clob_host, &signer).await?;

    tracing::info!("authenticated with Polymarket CLOB");
    println!("authenticated successfully");
    Ok(())
}
```

**Step 3: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add src/client.rs src/main.rs
git commit -m "feat: add Polymarket CLOB client authentication"
```

---

### Task 5: Output Formatting Module

**Files:**
- Create: `src/output.rs`

**Step 1: Create the output module**

Create `src/output.rs`:

```rust
use comfy_table::{ContentArrangement, Table};
use serde::Serialize;

pub fn print_output<T: Serialize>(json_mode: bool, headers: &[&str], rows: Vec<Vec<String>>, data: &T) {
    if json_mode {
        println!("{}", serde_json::to_string_pretty(data).unwrap());
    } else {
        let mut table = Table::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(headers);
        for row in rows {
            table.add_row(row);
        }
        println!("{table}");
    }
}

pub fn print_json<T: Serialize>(data: &T) {
    println!("{}", serde_json::to_string_pretty(data).unwrap());
}

pub fn print_error(json_mode: bool, msg: &str) {
    if json_mode {
        println!(r#"{{"error": "{}"}}"#, msg.replace('"', r#"\""#));
    } else {
        eprintln!("Error: {msg}");
    }
}
```

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles (even though it's not wired in yet, the compiler checks it as a module)

Note: Add `mod output;` to `main.rs` so the compiler picks it up.

**Step 3: Commit**

```bash
git add src/output.rs src/main.rs
git commit -m "feat: add output formatting module (table/JSON)"
```

---

### Task 6: Markets Commands

**Files:**
- Create: `src/commands/mod.rs`
- Create: `src/commands/markets.rs`
- Modify: `src/main.rs`

**Step 1: Create commands module structure**

Create `src/commands/mod.rs`:

```rust
pub mod markets;
```

Create `src/commands/markets.rs`:

```rust
use polymarket_client_sdk::auth::state::{Authenticated, State};
use polymarket_client_sdk::clob::Client;
use serde::Serialize;

use crate::output;

#[derive(Serialize)]
struct MarketSummary {
    condition_id: String,
    question: String,
    active: bool,
    tokens: Vec<TokenSummary>,
}

#[derive(Serialize)]
struct TokenSummary {
    outcome: String,
    token_id: String,
    price: String,
}

pub async fn list_markets<S: State>(client: &Client<S>, limit: usize, json: bool) -> anyhow::Result<()> {
    let mut all_markets = Vec::new();
    let mut cursor = None;

    loop {
        let page = client.simplified_markets(cursor).await?;
        all_markets.extend(page.data);
        if all_markets.len() >= limit || page.next_cursor.is_empty() {
            break;
        }
        cursor = Some(page.next_cursor);
    }

    all_markets.truncate(limit);

    let summaries: Vec<MarketSummary> = all_markets
        .iter()
        .map(|m| MarketSummary {
            condition_id: m.condition_id.clone(),
            question: String::new(), // SimplifiedMarketResponse may not have question
            active: m.active,
            tokens: m
                .tokens
                .iter()
                .map(|t| TokenSummary {
                    outcome: t.outcome.clone(),
                    token_id: t.token_id.to_string(),
                    price: t.price.to_string(),
                })
                .collect(),
        })
        .collect();

    if json {
        output::print_json(&summaries);
    } else {
        let headers = &["Condition ID", "Active", "Token", "Outcome", "Price"];
        let mut rows = Vec::new();
        for market in &summaries {
            for token in &market.tokens {
                rows.push(vec![
                    market.condition_id.chars().take(12).collect::<String>() + "...",
                    market.active.to_string(),
                    token.token_id.chars().take(12).collect::<String>() + "...",
                    token.outcome.clone(),
                    token.price.clone(),
                ]);
            }
        }
        output::print_output(false, headers, rows, &summaries);
    }

    Ok(())
}

pub async fn show_market<S: State>(client: &Client<S>, condition_id: &str, json: bool) -> anyhow::Result<()> {
    let market = client.market(condition_id).await?;

    let summary = MarketSummary {
        condition_id: market.condition_id.clone(),
        question: market.question.clone(),
        active: market.active,
        tokens: market
            .tokens
            .iter()
            .map(|t| TokenSummary {
                outcome: t.outcome.clone(),
                token_id: t.token_id.to_string(),
                price: t.price.to_string(),
            })
            .collect(),
    };

    if json {
        output::print_json(&summary);
    } else {
        println!("Market: {}", market.question);
        println!("Condition ID: {}", market.condition_id);
        println!("Active: {}", market.active);
        println!();
        let headers = &["Outcome", "Token ID", "Price"];
        let rows: Vec<Vec<String>> = summary
            .tokens
            .iter()
            .map(|t| vec![t.outcome.clone(), t.token_id.clone(), t.price.clone()])
            .collect();
        output::print_output(false, headers, rows, &summary);
    }

    Ok(())
}
```

**Step 2: Wire into main.rs with command dispatch**

Update `src/main.rs`:

```rust
mod cli;
mod client;
mod commands;
mod output;
mod signer;

use clap::Parser;
use cli::{Cli, Command, MarketsCommand};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let signer = signer::create_kms_signer(&cli.kms_key_id).await?;
    let client = client::create_authenticated_client(&cli.clob_host, &signer).await?;

    match cli.command {
        Command::Markets(args) => match args.command {
            MarketsCommand::List { limit } => {
                commands::markets::list_markets(&client, limit, cli.json).await?;
            }
            MarketsCommand::Show { condition_id } => {
                commands::markets::show_market(&client, &condition_id, cli.json).await?;
            }
        },
        _ => {
            println!("Command not yet implemented");
        }
    }

    Ok(())
}
```

**Step 3: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add src/commands/ src/main.rs
git commit -m "feat: add markets list and show commands"
```

---

### Task 7: Prices Commands

**Files:**
- Create: `src/commands/prices.rs`
- Modify: `src/commands/mod.rs`
- Modify: `src/main.rs`

**Step 1: Create prices command module**

Create `src/commands/prices.rs`:

```rust
use std::str::FromStr;

use polymarket_client_sdk::auth::state::State;
use polymarket_client_sdk::clob::types::request::{MidpointRequest, OrderBookSummaryRequest, SpreadRequest};
use polymarket_client_sdk::clob::Client;
use polymarket_client_sdk::types::U256;
use serde::Serialize;

use crate::output;

#[derive(Serialize)]
struct MidpointOutput {
    token_id: String,
    midpoint: String,
}

pub async fn midpoint<S: State>(client: &Client<S>, token_id_str: &str, json: bool) -> anyhow::Result<()> {
    let token_id = U256::from_str(token_id_str)?;
    let request = MidpointRequest::builder().token_id(token_id).build();
    let response = client.midpoint(&request).await?;

    let data = MidpointOutput {
        token_id: token_id_str.to_string(),
        midpoint: response.mid.to_string(),
    };

    if json {
        output::print_json(&data);
    } else {
        println!("Midpoint: {}", response.mid);
    }

    Ok(())
}

#[derive(Serialize)]
struct SpreadOutput {
    token_id: String,
    spread: String,
}

pub async fn spread<S: State>(client: &Client<S>, token_id_str: &str, json: bool) -> anyhow::Result<()> {
    let token_id = U256::from_str(token_id_str)?;
    let request = SpreadRequest::builder().token_id(token_id).build();
    let response = client.spread(&request).await?;

    let data = SpreadOutput {
        token_id: token_id_str.to_string(),
        spread: response.spread.to_string(),
    };

    if json {
        output::print_json(&data);
    } else {
        println!("Spread: {}", response.spread);
    }

    Ok(())
}

#[derive(Serialize)]
struct BookOutput {
    token_id: String,
    bids: Vec<LevelOutput>,
    asks: Vec<LevelOutput>,
}

#[derive(Serialize)]
struct LevelOutput {
    price: String,
    size: String,
}

pub async fn book<S: State>(client: &Client<S>, token_id_str: &str, json: bool) -> anyhow::Result<()> {
    let token_id = U256::from_str(token_id_str)?;
    let request = OrderBookSummaryRequest::builder().token_id(token_id).build();
    let response = client.order_book(&request).await?;

    let data = BookOutput {
        token_id: token_id_str.to_string(),
        bids: response
            .bids
            .iter()
            .map(|l| LevelOutput {
                price: l.price.to_string(),
                size: l.size.to_string(),
            })
            .collect(),
        asks: response
            .asks
            .iter()
            .map(|l| LevelOutput {
                price: l.price.to_string(),
                size: l.size.to_string(),
            })
            .collect(),
    };

    if json {
        output::print_json(&data);
    } else {
        println!("=== Order Book ===");
        println!();
        let headers = &["Side", "Price", "Size"];
        let mut rows = Vec::new();
        for ask in data.asks.iter().rev() {
            rows.push(vec!["ASK".to_string(), ask.price.clone(), ask.size.clone()]);
        }
        rows.push(vec!["---".to_string(), "---".to_string(), "---".to_string()]);
        for bid in &data.bids {
            rows.push(vec!["BID".to_string(), bid.price.clone(), bid.size.clone()]);
        }
        output::print_output(false, headers, rows, &data);
    }

    Ok(())
}
```

**Step 2: Add to commands/mod.rs**

```rust
pub mod markets;
pub mod prices;
```

**Step 3: Add price command dispatch to main.rs**

Add to the match in main.rs, after the Markets arm:

```rust
        Command::Prices(args) => match args.command {
            PricesCommand::Midpoint { token_id } => {
                commands::prices::midpoint(&client, &token_id, cli.json).await?;
            }
            PricesCommand::Spread { token_id } => {
                commands::prices::spread(&client, &token_id, cli.json).await?;
            }
            PricesCommand::Book { token_id } => {
                commands::prices::book(&client, &token_id, cli.json).await?;
            }
        },
```

Also add `PricesCommand` to the imports.

**Step 4: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully

**Step 5: Commit**

```bash
git add src/commands/prices.rs src/commands/mod.rs src/main.rs
git commit -m "feat: add prices midpoint, spread, and book commands"
```

---

### Task 8: Orders Commands

**Files:**
- Create: `src/commands/orders.rs`
- Modify: `src/commands/mod.rs`
- Modify: `src/main.rs`

**Step 1: Create orders command module**

Create `src/commands/orders.rs`:

```rust
use std::str::FromStr;

use alloy::signers::Signer;
use polymarket_client_sdk::auth::state::Authenticated;
use polymarket_client_sdk::auth::Normal;
use polymarket_client_sdk::clob::types::request::{CancelMarketOrderRequest, OrdersRequest};
use polymarket_client_sdk::clob::types::{Amount, Side};
use polymarket_client_sdk::clob::Client;
use polymarket_client_sdk::types::{Decimal, U256};
use rust_decimal_macros::dec;
use serde::Serialize;

use crate::output;

fn parse_side(s: &str) -> anyhow::Result<Side> {
    match s.to_lowercase().as_str() {
        "buy" => Ok(Side::Buy),
        "sell" => Ok(Side::Sell),
        _ => anyhow::bail!("Invalid side '{}'. Use 'buy' or 'sell'.", s),
    }
}

#[derive(Serialize)]
struct OrderOutput {
    id: String,
    market: String,
    side: String,
    price: String,
    original_size: String,
    size_matched: String,
    status: String,
}

pub async fn list_orders(
    client: &Client<Authenticated<Normal>>,
    _all: bool,
    json: bool,
) -> anyhow::Result<()> {
    let request = OrdersRequest::default();
    let page = client.orders(&request, None).await?;

    let orders: Vec<OrderOutput> = page
        .data
        .iter()
        .map(|o| OrderOutput {
            id: o.id.clone(),
            market: o.market.to_string(),
            side: format!("{:?}", o.side),
            price: o.price.to_string(),
            original_size: o.original_size.to_string(),
            size_matched: o.size_matched.to_string(),
            status: format!("{:?}", o.status),
        })
        .collect();

    if json {
        output::print_json(&orders);
    } else {
        let headers = &["ID", "Market", "Side", "Price", "Size", "Matched", "Status"];
        let rows: Vec<Vec<String>> = orders
            .iter()
            .map(|o| {
                vec![
                    o.id.chars().take(12).collect::<String>() + "...",
                    o.market.chars().take(12).collect::<String>() + "...",
                    o.side.clone(),
                    o.price.clone(),
                    o.original_size.clone(),
                    o.size_matched.clone(),
                    o.status.clone(),
                ]
            })
            .collect();
        output::print_output(false, headers, rows, &orders);
    }

    Ok(())
}

pub async fn place_limit<S2: Signer>(
    client: &Client<Authenticated<Normal>>,
    signer: &S2,
    token_id_str: &str,
    side_str: &str,
    price_str: &str,
    size_str: &str,
    json: bool,
) -> anyhow::Result<()> {
    let token_id = U256::from_str(token_id_str)?;
    let side = parse_side(side_str)?;
    let price = Decimal::from_str(price_str)?;
    let size = Decimal::from_str(size_str)?;

    let order = client
        .limit_order()
        .token_id(token_id)
        .side(side)
        .price(price)
        .size(size)
        .build()
        .await?;

    let signed = client.sign(signer, order).await?;
    let response = client.post_order(signed).await?;

    if json {
        output::print_json(&response);
    } else {
        if response.success {
            println!("Order placed: {}", response.order_id);
        } else {
            println!("Order failed: {}", response.error_msg.as_deref().unwrap_or("unknown error"));
        }
    }

    Ok(())
}

pub async fn place_market<S2: Signer>(
    client: &Client<Authenticated<Normal>>,
    signer: &S2,
    token_id_str: &str,
    side_str: &str,
    amount_str: &str,
    json: bool,
) -> anyhow::Result<()> {
    let token_id = U256::from_str(token_id_str)?;
    let side = parse_side(side_str)?;
    let amount_dec = Decimal::from_str(amount_str)?;
    let amount = Amount::usdc(amount_dec)?;

    let order = client
        .market_order()
        .token_id(token_id)
        .side(side)
        .amount(amount)
        .build()
        .await?;

    let signed = client.sign(signer, order).await?;
    let response = client.post_order(signed).await?;

    if json {
        output::print_json(&response);
    } else {
        if response.success {
            println!("Market order placed: {}", response.order_id);
        } else {
            println!("Order failed: {}", response.error_msg.as_deref().unwrap_or("unknown error"));
        }
    }

    Ok(())
}

pub async fn cancel_order(
    client: &Client<Authenticated<Normal>>,
    order_id: &str,
    json: bool,
) -> anyhow::Result<()> {
    let response = client.cancel_order(order_id).await?;

    if json {
        output::print_json(&response);
    } else {
        println!("Canceled: {:?}", response.canceled);
        if !response.not_canceled.is_empty() {
            println!("Not canceled: {:?}", response.not_canceled);
        }
    }

    Ok(())
}

pub async fn cancel_all(
    client: &Client<Authenticated<Normal>>,
    market: Option<&str>,
    json: bool,
) -> anyhow::Result<()> {
    let response = if let Some(market_id) = market {
        let request = CancelMarketOrderRequest::builder()
            .market(market_id.parse()?)
            .build();
        client.cancel_market_orders(&request).await?
    } else {
        client.cancel_all_orders().await?
    };

    if json {
        output::print_json(&response);
    } else {
        println!("Canceled: {} orders", response.canceled.len());
        if !response.not_canceled.is_empty() {
            println!("Not canceled: {:?}", response.not_canceled);
        }
    }

    Ok(())
}
```

**Step 2: Add to commands/mod.rs**

```rust
pub mod markets;
pub mod orders;
pub mod prices;
```

**Step 3: Add order command dispatch to main.rs**

Add to the match, after Prices arm. Note: orders need the signer reference for signing:

```rust
        Command::Orders(args) => match args.command {
            OrdersCommand::List { all } => {
                commands::orders::list_orders(&client, all, cli.json).await?;
            }
            OrdersCommand::Limit { token_id, side, price, size } => {
                commands::orders::place_limit(&client, &signer, &token_id, &side, &price, &size, cli.json).await?;
            }
            OrdersCommand::Market { token_id, side, amount } => {
                commands::orders::place_market(&client, &signer, &token_id, &side, &amount, cli.json).await?;
            }
            OrdersCommand::Cancel { order_id } => {
                commands::orders::cancel_order(&client, &order_id, cli.json).await?;
            }
            OrdersCommand::CancelAll { market } => {
                commands::orders::cancel_all(&client, market.as_deref(), cli.json).await?;
            }
        },
```

Also add `OrdersCommand` to the imports.

**Step 4: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully

**Step 5: Commit**

```bash
git add src/commands/orders.rs src/commands/mod.rs src/main.rs
git commit -m "feat: add order placement, listing, and cancellation commands"
```

---

### Task 9: Account Commands

**Files:**
- Create: `src/commands/account.rs`
- Modify: `src/commands/mod.rs`
- Modify: `src/main.rs`

**Step 1: Create account command module**

Create `src/commands/account.rs`:

```rust
use polymarket_client_sdk::auth::state::Authenticated;
use polymarket_client_sdk::auth::Normal;
use polymarket_client_sdk::clob::types::request::{BalanceAllowanceRequest, TradesRequest};
use polymarket_client_sdk::clob::Client;
use serde::Serialize;

use crate::output;

#[derive(Serialize)]
struct BalanceOutput {
    balance: String,
    allowances: std::collections::HashMap<String, String>,
}

pub async fn balance(
    client: &Client<Authenticated<Normal>>,
    json: bool,
) -> anyhow::Result<()> {
    let response = client
        .balance_allowance(BalanceAllowanceRequest::default())
        .await?;

    let data = BalanceOutput {
        balance: response.balance.to_string(),
        allowances: response
            .allowances
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    };

    if json {
        output::print_json(&data);
    } else {
        println!("Balance: {} USDC", response.balance);
        if !response.allowances.is_empty() {
            println!("Allowances:");
            for (addr, amount) in &response.allowances {
                println!("  {}: {}", addr, amount);
            }
        }
    }

    Ok(())
}

#[derive(Serialize)]
struct TradeOutput {
    id: String,
    market: String,
    side: String,
    price: String,
    size: String,
    status: String,
}

pub async fn trades(
    client: &Client<Authenticated<Normal>>,
    limit: usize,
    json: bool,
) -> anyhow::Result<()> {
    let request = TradesRequest::default();
    let page = client.trades(&request, None).await?;

    let mut trades_out: Vec<TradeOutput> = page
        .data
        .iter()
        .map(|t| TradeOutput {
            id: t.id.clone(),
            market: t.market.to_string(),
            side: format!("{:?}", t.side),
            price: t.price.to_string(),
            size: t.size.to_string(),
            status: format!("{:?}", t.status),
        })
        .collect();

    trades_out.truncate(limit);

    if json {
        output::print_json(&trades_out);
    } else {
        let headers = &["ID", "Market", "Side", "Price", "Size", "Status"];
        let rows: Vec<Vec<String>> = trades_out
            .iter()
            .map(|t| {
                vec![
                    t.id.chars().take(12).collect::<String>() + "...",
                    t.market.chars().take(12).collect::<String>() + "...",
                    t.side.clone(),
                    t.price.clone(),
                    t.size.clone(),
                    t.status.clone(),
                ]
            })
            .collect();
        output::print_output(false, headers, rows, &trades_out);
    }

    Ok(())
}
```

**Step 2: Add to commands/mod.rs**

```rust
pub mod account;
pub mod markets;
pub mod orders;
pub mod prices;
```

**Step 3: Add account command dispatch to main.rs**

Add to the match, replacing the `_ =>` arm:

```rust
        Command::Account(args) => match args.command {
            AccountCommand::Balance => {
                commands::account::balance(&client, cli.json).await?;
            }
            AccountCommand::Trades { limit } => {
                commands::account::trades(&client, limit, cli.json).await?;
            }
        },
```

Also add `AccountCommand` to the imports.

**Step 4: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully

**Step 5: Commit**

```bash
git add src/commands/account.rs src/commands/mod.rs src/main.rs
git commit -m "feat: add account balance and trades commands"
```

---

### Task 10: Unauthenticated Command Optimization

Some commands (markets, prices) don't need authentication. This task optimizes the CLI to skip auth for read-only commands.

**Files:**
- Modify: `src/main.rs`

**Step 1: Refactor main.rs to conditionally authenticate**

```rust
mod cli;
mod client;
mod commands;
mod output;
mod signer;

use clap::Parser;
use cli::{AccountCommand, Cli, Command, MarketsCommand, OrdersCommand, PricesCommand};
use tracing_subscriber::EnvFilter;

fn needs_auth(command: &Command) -> bool {
    matches!(command, Command::Orders(_) | Command::Account(_))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    if needs_auth(&cli.command) {
        let signer = signer::create_kms_signer(&cli.kms_key_id).await?;
        let client = client::create_authenticated_client(&cli.clob_host, &signer).await?;

        match cli.command {
            Command::Orders(args) => match args.command {
                OrdersCommand::List { all } => {
                    commands::orders::list_orders(&client, all, cli.json).await?;
                }
                OrdersCommand::Limit { token_id, side, price, size } => {
                    commands::orders::place_limit(&client, &signer, &token_id, &side, &price, &size, cli.json).await?;
                }
                OrdersCommand::Market { token_id, side, amount } => {
                    commands::orders::place_market(&client, &signer, &token_id, &side, &amount, cli.json).await?;
                }
                OrdersCommand::Cancel { order_id } => {
                    commands::orders::cancel_order(&client, &order_id, cli.json).await?;
                }
                OrdersCommand::CancelAll { market } => {
                    commands::orders::cancel_all(&client, market.as_deref(), cli.json).await?;
                }
            },
            Command::Account(args) => match args.command {
                AccountCommand::Balance => {
                    commands::account::balance(&client, cli.json).await?;
                }
                AccountCommand::Trades { limit } => {
                    commands::account::trades(&client, limit, cli.json).await?;
                }
            },
            _ => unreachable!(),
        }
    } else {
        let client = client::create_unauthenticated_client(&cli.clob_host)?;

        match cli.command {
            Command::Markets(args) => match args.command {
                MarketsCommand::List { limit } => {
                    commands::markets::list_markets(&client, limit, cli.json).await?;
                }
                MarketsCommand::Show { condition_id } => {
                    commands::markets::show_market(&client, &condition_id, cli.json).await?;
                }
            },
            Command::Prices(args) => match args.command {
                PricesCommand::Midpoint { token_id } => {
                    commands::prices::midpoint(&client, &token_id, cli.json).await?;
                }
                PricesCommand::Spread { token_id } => {
                    commands::prices::spread(&client, &token_id, cli.json).await?;
                }
                PricesCommand::Book { token_id } => {
                    commands::prices::book(&client, &token_id, cli.json).await?;
                }
            },
            _ => unreachable!(),
        }
    }

    Ok(())
}
```

**Step 2: Add unauthenticated client constructor to client.rs**

Add to `src/client.rs`:

```rust
use polymarket_client_sdk::auth::state::Unauthenticated;

pub fn create_unauthenticated_client(host: &str) -> anyhow::Result<Client<Unauthenticated>> {
    let client = Client::new(host, Config::default())?;
    Ok(client)
}
```

**Step 3: Make kms_key_id optional in cli.rs**

Change the `kms_key_id` field:

```rust
    #[arg(long, env = "POLYMARKET_KMS_KEY_ID", global = true)]
    pub kms_key_id: Option<String>,
```

And in the auth path in main.rs, extract with:

```rust
let kms_key_id = cli.kms_key_id.as_deref()
    .ok_or_else(|| anyhow::anyhow!("--kms-key-id or POLYMARKET_KMS_KEY_ID required for this command"))?;
let signer = signer::create_kms_signer(kms_key_id).await?;
```

**Step 4: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully

**Step 5: Commit**

```bash
git add src/main.rs src/client.rs src/cli.rs
git commit -m "feat: skip auth for read-only commands (markets, prices)"
```

---

### Task 11: Error Handling Polish

**Files:**
- Modify: `src/main.rs`

**Step 1: Add structured error output**

Wrap the main function body in error handling that respects `--json` mode:

```rust
#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    if let Err(e) = run(cli).await {
        // Check if json mode was requested (re-parse since cli is moved)
        let json = std::env::args().any(|a| a == "--json");
        if json {
            output::print_error(true, &format!("{e:#}"));
        } else {
            output::print_error(false, &format!("{e:#}"));
        }
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    // ... move all the dispatch logic here ...
}
```

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: add structured error handling with JSON support"
```

---

### Task 12: Build Verification and Cleanup

**Files:**
- All files

**Step 1: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: No warnings

**Step 2: Fix any clippy warnings**

Address each warning individually.

**Step 3: Run cargo fmt**

Run: `cargo fmt`

**Step 4: Final build test**

Run: `cargo build --release`
Expected: Compiles successfully

**Step 5: Test --help output**

Run: `cargo run --release -- --help`
Expected: Shows well-formatted help with all subcommands

Run: `cargo run --release -- markets --help`
Run: `cargo run --release -- orders --help`
Run: `cargo run --release -- prices --help`
Run: `cargo run --release -- account --help`

**Step 6: Commit any cleanup**

```bash
git add -A
git commit -m "chore: clippy fixes and formatting"
```
