mod cli;
mod client;
mod commands;
mod dry_run;
mod gamma;
mod output;
mod resolve;
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
                    include_closed,
                    sort,
                    min_volume,
                } => {
                    commands::markets::list_markets(
                        &gamma_client,
                        *limit,
                        query.as_deref(),
                        *include_closed,
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
                        false,
                        "volume_24hr",
                        None,
                        json,
                    )
                    .await?;
                }
                MarketsCommand::Watch {
                    markets,
                    outcome,
                    interval,
                } => {
                    let clob_client = client::create_unauthenticated_client(&cli.clob_host)?;
                    let mut resolved_markets = Vec::new();
                    for m in markets {
                        let resolved =
                            resolve::resolve_market(&gamma_client, m, outcome.as_deref()).await?;
                        resolved_markets.push(resolved);
                    }
                    commands::watch::watch(&clob_client, &resolved_markets, *interval, json)
                        .await?;
                }
            }
        }
        Command::Prices(args) => {
            let clob_client = client::create_unauthenticated_client(&cli.clob_host)?;
            let gamma_client = gamma::create_gamma_client();
            match &args.command {
                PricesCommand::Midpoint { market, outcome } => {
                    let resolved =
                        resolve::resolve_market(&gamma_client, market, outcome.as_deref()).await?;
                    commands::prices::midpoint(&clob_client, &resolved.token_id_str, json).await?;
                }
                PricesCommand::Spread { market, outcome } => {
                    let resolved =
                        resolve::resolve_market(&gamma_client, market, outcome.as_deref()).await?;
                    commands::prices::spread(&clob_client, &resolved.token_id_str, json).await?;
                }
                PricesCommand::Book { market, outcome } => {
                    let resolved =
                        resolve::resolve_market(&gamma_client, market, outcome.as_deref()).await?;
                    commands::prices::book(&clob_client, &resolved.token_id_str, json).await?;
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

            let gamma_client = gamma::create_gamma_client();

            match &args.command {
                OrdersCommand::List { all } => {
                    commands::orders::list_orders(&client, *all, json).await?;
                }
                OrdersCommand::Limit {
                    market,
                    outcome,
                    side,
                    price,
                    size,
                } => {
                    let resolved =
                        resolve::resolve_market(&gamma_client, market, outcome.as_deref()).await?;
                    commands::orders::place_limit(
                        &client,
                        &kms_signer,
                        &resolved.token_id_str,
                        side,
                        price,
                        size,
                        json,
                    )
                    .await?;
                }
                OrdersCommand::Market {
                    market,
                    outcome,
                    side,
                    amount,
                } => {
                    let resolved =
                        resolve::resolve_market(&gamma_client, market, outcome.as_deref()).await?;
                    commands::orders::place_market(
                        &client,
                        &kms_signer,
                        &resolved.token_id_str,
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
            let clob_client = client::create_unauthenticated_client(&cli.clob_host)?;
            let gamma_client = gamma::create_gamma_client();
            match &args.command {
                DryRunCommand::Limit {
                    market,
                    outcome,
                    side,
                    price,
                    size,
                } => {
                    let resolved =
                        resolve::resolve_market(&gamma_client, market, outcome.as_deref()).await?;
                    commands::dry_run::place_limit(
                        &clob_client,
                        &resolved,
                        side,
                        price,
                        size,
                        json,
                    )
                    .await?;
                }
                DryRunCommand::Market {
                    market,
                    outcome,
                    side,
                    amount,
                } => {
                    let resolved =
                        resolve::resolve_market(&gamma_client, market, outcome.as_deref()).await?;
                    commands::dry_run::place_market(&clob_client, &resolved, side, amount, json)
                        .await?;
                }
                DryRunCommand::Close {
                    market,
                    outcome,
                    size,
                } => {
                    let resolved =
                        resolve::resolve_market(&gamma_client, market, outcome.as_deref()).await?;
                    commands::dry_run::close(&clob_client, &resolved, size.as_deref(), json)
                        .await?;
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
                    commands::dry_run::pnl(&clob_client, json).await?;
                }
                DryRunCommand::Portfolio => {
                    commands::dry_run::portfolio(&clob_client, json).await?;
                }
                DryRunCommand::Reset { balance } => {
                    commands::dry_run::reset(balance, json)?;
                }
            }
        }
    }

    Ok(())
}
