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
                        "volume",
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
                    let futs: Vec<_> = markets
                        .iter()
                        .map(|m| resolve::resolve_market(&gamma_client, m, outcome.as_deref()))
                        .collect();
                    let resolved_markets = futures::future::try_join_all(futs).await?;
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
            let signer = match (&cli.private_key, &cli.kms_key_id) {
                (Some(_), Some(_)) => {
                    anyhow::bail!("Cannot specify both --private-key and --kms-key-id")
                }
                (Some(pk), None) => signer::AnySigner::Local(signer::create_local_signer(pk)?),
                (None, Some(key_id)) => {
                    signer::AnySigner::Kms(signer::create_kms_signer(key_id).await?)
                }
                (None, None) => anyhow::bail!(
                    "Wallet key is required for this command. \
                     Set --private-key / POLYMARKET_PRIVATE_KEY or \
                     --kms-key-id / POLYMARKET_KMS_KEY_ID."
                ),
            };
            let client = client::create_authenticated_client(&cli.clob_host, &signer).await?;

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
                        &signer,
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
                        &signer,
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
            let signer = match (&cli.private_key, &cli.kms_key_id) {
                (Some(_), Some(_)) => {
                    anyhow::bail!("Cannot specify both --private-key and --kms-key-id")
                }
                (Some(pk), None) => signer::AnySigner::Local(signer::create_local_signer(pk)?),
                (None, Some(key_id)) => {
                    signer::AnySigner::Kms(signer::create_kms_signer(key_id).await?)
                }
                (None, None) => anyhow::bail!(
                    "Wallet key is required for this command. \
                     Set --private-key / POLYMARKET_PRIVATE_KEY or \
                     --kms-key-id / POLYMARKET_KMS_KEY_ID."
                ),
            };
            let client = client::create_authenticated_client(&cli.clob_host, &signer).await?;

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
                    commands::dry_run::print_outcome_note(&resolved, outcome.is_some());
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
                    commands::dry_run::print_outcome_note(&resolved, outcome.is_some());
                }
                DryRunCommand::Close {
                    market,
                    outcome,
                    size,
                } => {
                    let market_identifier = if let Ok(index) = market.parse::<usize>() {
                        if index > 0 && index <= 1000 {
                            let (token_id, slug) =
                                commands::dry_run::resolve_position_index(index)?;
                            slug.unwrap_or(token_id)
                        } else {
                            market.clone()
                        }
                    } else {
                        market.clone()
                    };
                    let resolved = resolve::resolve_market(
                        &gamma_client,
                        &market_identifier,
                        outcome.as_deref(),
                    )
                    .await?;
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
                DryRunCommand::Portfolio {
                    take_profit,
                    stop_loss,
                } => {
                    commands::dry_run::portfolio(&clob_client, *take_profit, *stop_loss, json)
                        .await?;
                }
                DryRunCommand::Summary => {
                    commands::dry_run::summary(&clob_client, json).await?;
                }
                DryRunCommand::Alerts {
                    take_profit,
                    stop_loss,
                    interval,
                } => {
                    commands::dry_run::alerts(
                        &clob_client,
                        *take_profit,
                        *stop_loss,
                        *interval,
                        json,
                    )
                    .await?;
                }
                DryRunCommand::Reset { balance } => {
                    commands::dry_run::reset(balance, json)?;
                }
            }
        }
    }

    Ok(())
}
