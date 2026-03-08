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

#[derive(Parser)]
pub struct MarketsArgs {
    #[command(subcommand)]
    pub command: MarketsCommand,
}

#[derive(Subcommand)]
pub enum MarketsCommand {
    /// List active markets
    List {
        #[arg(long, default_value = "25")]
        limit: usize,
    },
    /// Show market details
    Show {
        /// Condition ID of the market
        condition_id: String,
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
