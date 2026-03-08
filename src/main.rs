mod cli;
mod client;
mod commands;
mod output;
mod signer;

use clap::Parser;
use cli::{AccountCommand, Cli, Command, MarketsCommand, OrdersCommand, PricesCommand};
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
            let client = client::create_unauthenticated_client(&cli.clob_host)?;
            match &args.command {
                MarketsCommand::List { limit } => {
                    commands::markets::list_markets(&client, *limit, json).await?;
                }
                MarketsCommand::Show { condition_id } => {
                    commands::markets::show_market(&client, condition_id, json).await?;
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
    }

    Ok(())
}
