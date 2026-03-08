mod cli;
mod client;
mod commands;
mod dry_run;
mod gamma;
mod output;
mod signer;

use clap::Parser;
use cli::{
    AccountCommand, Cli, Command, DryRunCommand, MarketsCommand, OrdersCommand, PricesCommand,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let json = cli.json;

    if let Err(e) = run(cli).await {
        output::print_error(json, &format!("{e:#}"));
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let json = cli.json;

    match &cli.command {
        // Markets and Prices don't require authentication
        Command::Markets(args) => {
            let gamma_client = gamma::create_gamma_client();
            match &args.command {
                MarketsCommand::List {
                    limit,
                    query,
                    active,
                    sort,
                    min_volume,
                } => {
                    commands::markets::list_markets(
                        &gamma_client,
                        *limit,
                        query.as_deref(),
                        *active,
                        sort,
                        min_volume.as_deref(),
                        json,
                    )
                    .await?;
                }
                MarketsCommand::Show { market } => {
                    let clob_client = client::create_unauthenticated_client(&cli.clob_host)?;
                    commands::markets::show_market(&gamma_client, &clob_client, market, json)
                        .await?;
                }
                MarketsCommand::Trending { limit } => {
                    commands::markets::list_markets(
                        &gamma_client,
                        *limit,
                        None,
                        true,
                        "volume_24hr",
                        None,
                        json,
                    )
                    .await?;
                }
            }
        }
        Command::Prices(args) => {
            let client = client::create_unauthenticated_client(&cli.clob_host)?;
            match &args.command {
                PricesCommand::Midpoint { token_id } => {
                    commands::prices::midpoint(&client, token_id, json).await?;
                }
                PricesCommand::Spread { token_id } => {
                    commands::prices::spread(&client, token_id, json).await?;
                }
                PricesCommand::Book { token_id } => {
                    commands::prices::book(&client, token_id, json).await?;
                }
            }
        }
        // Orders and Account require authentication
        Command::Orders(args) => {
            let kms_key_id = cli.kms_key_id.as_deref().ok_or_else(|| {
                anyhow::anyhow!(
                    "KMS key ID is required for order commands. \
                     Set --kms-key-id or POLYMARKET_KMS_KEY_ID env var."
                )
            })?;
            let kms_signer = signer::create_kms_signer(kms_key_id).await?;
            let client = client::create_authenticated_client(&cli.clob_host, &kms_signer).await?;

            match &args.command {
                OrdersCommand::List { all } => {
                    commands::orders::list_orders(&client, *all, json).await?;
                }
                OrdersCommand::Limit {
                    token_id,
                    side,
                    price,
                    size,
                } => {
                    commands::orders::place_limit(
                        &client,
                        &kms_signer,
                        token_id,
                        side,
                        price,
                        size,
                        json,
                    )
                    .await?;
                }
                OrdersCommand::Market {
                    token_id,
                    side,
                    amount,
                } => {
                    commands::orders::place_market(
                        &client,
                        &kms_signer,
                        token_id,
                        side,
                        amount,
                        json,
                    )
                    .await?;
                }
                OrdersCommand::Cancel { order_id } => {
                    commands::orders::cancel_order(&client, order_id, json).await?;
                }
                OrdersCommand::CancelAll { market } => {
                    commands::orders::cancel_all(&client, market.as_deref(), json).await?;
                }
            }
        }
        Command::Account(args) => {
            let kms_key_id = cli.kms_key_id.as_deref().ok_or_else(|| {
                anyhow::anyhow!(
                    "KMS key ID is required for account commands. \
                     Set --kms-key-id or POLYMARKET_KMS_KEY_ID env var."
                )
            })?;
            let kms_signer = signer::create_kms_signer(kms_key_id).await?;
            let client = client::create_authenticated_client(&cli.clob_host, &kms_signer).await?;

            match &args.command {
                AccountCommand::Balance => {
                    commands::account::balance(&client, json).await?;
                }
                AccountCommand::Trades { limit } => {
                    commands::account::trades(&client, *limit, json).await?;
                }
            }
        }
        Command::DryRun(args) => {
            let client = client::create_unauthenticated_client(&cli.clob_host)?;
            match &args.command {
                DryRunCommand::Limit {
                    token_id,
                    side,
                    price,
                    size,
                } => {
                    commands::dry_run::place_limit(&client, token_id, side, price, size, json)
                        .await?;
                }
                DryRunCommand::Market {
                    token_id,
                    side,
                    amount,
                } => {
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
    }

    Ok(())
}
