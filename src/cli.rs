use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "polymarket-trader", about = "Polymarket trading CLI")]
pub struct Cli {
    /// Output as JSON instead of tables
    #[arg(long, global = true)]
    pub json: bool,

    /// AWS KMS key ID for wallet signing
    #[arg(long, env = "POLYMARKET_KMS_KEY_ID", global = true)]
    pub kms_key_id: Option<String>,

    /// Polymarket CLOB API host
    #[arg(
        long,
        default_value = "https://clob.polymarket.com",
        env = "POLYMARKET_CLOB_HOST",
        global = true
    )]
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
    /// Simulated trading (paper trading) commands
    DryRun(DryRunArgs),
}

#[derive(Parser)]
pub struct MarketsArgs {
    #[command(subcommand)]
    pub command: MarketsCommand,
}

#[derive(Subcommand)]
pub enum MarketsCommand {
    /// List active markets (uses Gamma API)
    List {
        /// Maximum number of results
        #[arg(long, default_value = "25")]
        limit: usize,
        /// Search query text
        #[arg(long)]
        query: Option<String>,
        /// Show only active markets (default: true)
        #[arg(long, default_value = "true")]
        active: bool,
        /// Sort by: volume, volume_24hr, liquidity, created_at
        #[arg(long, default_value = "volume")]
        sort: String,
        /// Minimum total volume filter
        #[arg(long)]
        min_volume: Option<String>,
    },
    /// Show market details (accepts condition ID or slug)
    Show {
        /// Condition ID or slug of the market
        market: String,
    },
    /// Show trending markets (top by 24h volume)
    Trending {
        /// Maximum number of results
        #[arg(long, default_value = "10")]
        limit: usize,
    },
}

#[derive(Parser)]
pub struct PricesArgs {
    #[command(subcommand)]
    pub command: PricesCommand,
}

#[derive(Subcommand)]
pub enum PricesCommand {
    /// Get current midpoint price
    Midpoint { token_id: String },
    /// Get bid-ask spread
    Spread { token_id: String },
    /// Get full order book
    Book { token_id: String },
}

#[derive(Parser)]
pub struct OrdersArgs {
    #[command(subcommand)]
    pub command: OrdersCommand,
}

#[derive(Subcommand)]
pub enum OrdersCommand {
    /// List your orders
    List {
        #[arg(long)]
        all: bool,
    },
    /// Place a limit order
    Limit {
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
        token_id: String,
        /// Side: "buy" or "sell"
        side: String,
        /// Amount in USDC
        amount: String,
    },
    /// Cancel an order by ID
    Cancel { order_id: String },
    /// Cancel all open orders
    CancelAll {
        #[arg(long)]
        market: Option<String>,
    },
}

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
        #[arg(long, default_value = "25")]
        limit: usize,
    },
}

#[derive(Parser)]
pub struct DryRunArgs {
    #[command(subcommand)]
    pub command: DryRunCommand,
}

#[derive(Subcommand)]
pub enum DryRunCommand {
    /// Simulate a limit order (fills at current midpoint)
    Limit {
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
        token_id: String,
        /// Side: "buy" or "sell"
        side: String,
        /// Amount in USDC
        amount: String,
    },
    /// Remove a simulated trade
    Cancel { trade_id: String },
    /// Show current simulated positions
    Positions,
    /// Show simulated trade history
    Trades {
        #[arg(long, default_value = "25")]
        limit: usize,
    },
    /// Show profit and loss report
    Pnl,
    /// Reset dry-run state
    Reset {
        #[arg(long, default_value = "1000.00")]
        balance: String,
    },
}
