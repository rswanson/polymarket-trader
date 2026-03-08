use std::collections::HashMap;
use std::str::FromStr;

use anyhow::{Context, Result};
use chrono::Utc;
use futures::future::try_join_all;
use polymarket_client_sdk::auth::state::State;
use polymarket_client_sdk::clob::Client;
use polymarket_client_sdk::clob::types::Side;
use polymarket_client_sdk::clob::types::request::MidpointRequest;
use polymarket_client_sdk::types::Decimal;
use serde::Serialize;
use uuid::Uuid;

use super::{parse_side, parse_token_id};
use crate::dry_run::db::{DryRunDb, MarketMetadata, Trade};
use crate::dry_run::portfolio;
use crate::output::print_output;
use crate::resolve::ResolvedMarket;

fn truncate_token_id(token_id: &str) -> String {
    crate::output::truncate(token_id, 12)
}

async fn fetch_midpoint<S: State>(client: &Client<S>, token_id_str: &str) -> Result<Decimal> {
    let token_id = parse_token_id(token_id_str)?;
    let request = MidpointRequest::builder().token_id(token_id).build();
    let response = client
        .midpoint(&request)
        .await
        .context("Failed to fetch midpoint")?;
    Ok(response.mid)
}

/// Look up market and outcome display names from metadata.
fn display_names(metadata: &HashMap<String, MarketMetadata>, token_id: &str) -> (String, String) {
    let meta = metadata.get(token_id);
    let market_name = meta
        .and_then(|m| m.question.as_deref())
        .map(|q| crate::output::truncate(q, 40))
        .unwrap_or_else(|| truncate_token_id(token_id));
    let outcome_name = meta
        .and_then(|m| m.outcome.as_deref())
        .unwrap_or("-")
        .to_string();
    (market_name, outcome_name)
}

struct PnlData {
    report: portfolio::PnlReport,
    metadata: HashMap<String, MarketMetadata>,
    all_trades: Vec<Trade>,
}

async fn build_pnl_data<S: State>(client: &Client<S>) -> Result<PnlData> {
    let db = DryRunDb::open()?;
    let all_trades = db.all_trades()?;
    let positions = portfolio::compute_positions(&all_trades)?;
    let metadata = db.all_metadata()?;

    let futs: Vec<_> = positions
        .iter()
        .map(|pos| async move {
            let mid = fetch_midpoint(client, &pos.token_id).await?;
            Ok::<_, anyhow::Error>((pos.token_id.clone(), mid))
        })
        .collect();
    let current_prices: HashMap<String, Decimal> = try_join_all(futs).await?.into_iter().collect();

    let starting_balance = db.get_starting_balance()?;
    let current_balance = db.get_balance()?;
    let report = portfolio::compute_pnl(
        &positions,
        &current_prices,
        &starting_balance,
        &current_balance,
    )?;

    Ok(PnlData {
        report,
        metadata,
        all_trades,
    })
}

#[derive(Serialize)]
struct TradeResult {
    id: String,
    token_id: String,
    side: String,
    price: String,
    size: String,
    cost: String,
    balance: String,
}

/// Shared logic for recording a dry-run trade (used by both place_limit and place_market).
fn record_trade(
    db: &DryRunDb,
    resolved: &ResolvedMarket,
    side: Side,
    cost: Decimal,
    size: Decimal,
    midpoint: Decimal,
    json: bool,
) -> Result<()> {
    let token_id = &resolved.token_id_str;
    let mut balance =
        Decimal::from_str(&db.get_balance()?).context("invalid balance in database")?;
    let side_str = match side {
        Side::Buy => "buy",
        _ => "sell",
    };

    if side == Side::Buy {
        anyhow::ensure!(
            balance >= cost,
            "Insufficient balance: have {balance}, need {cost}"
        );
        balance -= cost;
    } else {
        let held_dec = db.net_position_size(token_id)?;
        anyhow::ensure!(
            held_dec >= size,
            "Insufficient position: hold {held_dec} shares, trying to sell {size}"
        );
        balance += cost;
    }

    let balance = balance.round_dp(2);

    let trade = Trade {
        id: Uuid::new_v4().to_string(),
        token_id: token_id.to_string(),
        side: side_str.to_string(),
        price: midpoint.round_dp(6).to_string(),
        size: size.to_string(),
        cost: cost.to_string(),
        timestamp: Utc::now().to_rfc3339(),
    };

    db.insert_trade(&trade)?;
    db.upsert_metadata(
        token_id,
        resolved.slug.as_deref(),
        resolved.question.as_deref(),
        resolved.outcome.as_deref(),
    )?;
    db.update_balance(&balance.to_string())?;

    let result = TradeResult {
        id: trade.id.clone(),
        token_id: trade.token_id.clone(),
        side: trade.side.clone(),
        price: trade.price.clone(),
        size: trade.size.clone(),
        cost: trade.cost.clone(),
        balance: balance.to_string(),
    };

    let headers = &["ID", "Token", "Side", "Price", "Size", "Cost", "Balance"];
    let rows = vec![vec![
        result.id.clone(),
        truncate_token_id(&result.token_id),
        result.side.clone(),
        result.price.clone(),
        result.size.clone(),
        result.cost.clone(),
        result.balance.clone(),
    ]];
    print_output(json, headers, rows, &result);

    Ok(())
}

/// Resolve a 1-based position index to a (token_id, optional slug) pair.
pub fn resolve_position_index(index: usize) -> Result<(String, Option<String>)> {
    let db = DryRunDb::open()?;
    let trades = db.all_trades()?;
    let positions = portfolio::compute_positions(&trades)?;
    let pos = positions
        .get(
            index
                .checked_sub(1)
                .context("Position index must be >= 1")?,
        )
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Position index {index} out of range. You have {} open positions.",
                positions.len()
            )
        })?;
    let metadata = db.get_metadata(&pos.token_id)?;
    let slug = metadata.and_then(|m| m.slug);
    Ok((pos.token_id.clone(), slug))
}

pub async fn close<S: State>(
    client: &Client<S>,
    resolved: &ResolvedMarket,
    size: Option<&str>,
    json: bool,
) -> Result<()> {
    let db = DryRunDb::open()?;
    let held_dec = db.net_position_size(&resolved.token_id_str)?;

    anyhow::ensure!(held_dec > Decimal::ZERO, "No open position for this market");

    let sell_size = match size {
        Some(s) => {
            let requested =
                Decimal::from_str(s).map_err(|e| anyhow::anyhow!("Invalid size: {e}"))?;
            anyhow::ensure!(
                requested <= held_dec,
                "Requested size {requested} exceeds held position {held_dec}"
            );
            requested
        }
        None => held_dec,
    };

    let midpoint = fetch_midpoint(client, &resolved.token_id_str).await?;
    let cost = (midpoint * sell_size).round_dp(2);

    record_trade(&db, resolved, Side::Sell, cost, sell_size, midpoint, json)
}

pub async fn place_limit<S: State>(
    client: &Client<S>,
    resolved: &ResolvedMarket,
    side_str: &str,
    _price_str: &str,
    size_str: &str,
    json: bool,
) -> Result<()> {
    let side = parse_side(side_str)?;
    let size = Decimal::from_str(size_str).map_err(|e| anyhow::anyhow!("Invalid size: {e}"))?;
    let midpoint = fetch_midpoint(client, &resolved.token_id_str).await?;
    let cost = (midpoint * size).round_dp(2);

    let db = DryRunDb::open()?;
    record_trade(&db, resolved, side, cost, size, midpoint, json)
}

pub async fn place_market<S: State>(
    client: &Client<S>,
    resolved: &ResolvedMarket,
    side_str: &str,
    amount_str: &str,
    json: bool,
) -> Result<()> {
    let side = parse_side(side_str)?;
    let amount =
        Decimal::from_str(amount_str).map_err(|e| anyhow::anyhow!("Invalid amount: {e}"))?;
    let midpoint = fetch_midpoint(client, &resolved.token_id_str).await?;
    anyhow::ensure!(
        !midpoint.is_zero(),
        "Midpoint is zero for token {}, cannot calculate size",
        resolved.token_id_str
    );
    let size = amount / midpoint;
    let cost = amount.round_dp(2);

    let db = DryRunDb::open()?;
    record_trade(&db, resolved, side, cost, size, midpoint, json)
}

#[derive(Serialize)]
struct CancelResult {
    trade_id: String,
    canceled: bool,
    balance: String,
}

pub fn cancel(trade_id: &str, json: bool) -> Result<()> {
    let db = DryRunDb::open()?;
    let trade = db
        .delete_trade(trade_id)?
        .ok_or_else(|| anyhow::anyhow!("Trade '{trade_id}' not found"))?;

    let mut balance =
        Decimal::from_str(&db.get_balance()?).context("invalid balance in database")?;

    let cost = Decimal::from_str(&trade.cost).context("invalid cost in trade")?;
    if trade.side == "buy" {
        balance += cost;
    } else {
        balance -= cost;
    }
    let balance = balance.round_dp(2);
    db.update_balance(&balance.to_string())?;

    let result = CancelResult {
        trade_id: trade_id.to_string(),
        canceled: true,
        balance: balance.to_string(),
    };

    let headers = &["Trade ID", "Canceled", "Balance"];
    let rows = vec![vec![
        result.trade_id.clone(),
        result.canceled.to_string(),
        result.balance.clone(),
    ]];
    print_output(json, headers, rows, &result);

    Ok(())
}

pub fn positions(json: bool) -> Result<()> {
    let db = DryRunDb::open()?;
    let trades = db.all_trades()?;
    let positions = portfolio::compute_positions(&trades)?;
    let metadata = db.all_metadata()?;

    let headers = &[
        "#",
        "Market",
        "Outcome",
        "Side",
        "Size",
        "Avg Price",
        "Total Cost",
    ];
    let rows: Vec<Vec<String>> = positions
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let (market_name, outcome_name) = display_names(&metadata, &p.token_id);
            vec![
                (i + 1).to_string(),
                market_name,
                outcome_name,
                p.side.clone(),
                p.net_size.clone(),
                p.avg_price.clone(),
                p.total_cost.clone(),
            ]
        })
        .collect();
    print_output(json, headers, rows, &positions);

    Ok(())
}

pub fn trades(limit: usize, json: bool) -> Result<()> {
    let db = DryRunDb::open()?;
    let trades = db.list_trades(limit)?;
    let metadata = db.all_metadata()?;

    let headers = &[
        "ID", "Market", "Outcome", "Side", "Price", "Size", "Cost", "Time",
    ];
    let rows: Vec<Vec<String>> = trades
        .iter()
        .map(|t| {
            let (market_name, outcome_name) = display_names(&metadata, &t.token_id);
            vec![
                t.id.clone(),
                market_name,
                outcome_name,
                t.side.clone(),
                t.price.clone(),
                t.size.clone(),
                t.cost.clone(),
                t.timestamp.clone(),
            ]
        })
        .collect();
    print_output(json, headers, rows, &trades);

    Ok(())
}

pub async fn pnl<S: State>(client: &Client<S>, json: bool) -> Result<()> {
    let data = build_pnl_data(client).await?;
    let report = &data.report;

    if json {
        print_output(true, &[], vec![], report);
    } else {
        println!("Starting Balance: ${}", report.starting_balance);
        println!("Cash:             ${}", report.current_balance);
        println!("Position Value:   ${}", report.position_value);
        println!("Total Value:      ${}", report.total_value);
        println!("Net P&L:          ${}", report.total_pnl);
        println!();

        let headers = &[
            "Market",
            "Outcome",
            "Side",
            "Size",
            "Avg Price",
            "Current Price",
            "Value",
            "Unrealized P&L",
        ];
        let rows: Vec<Vec<String>> = report
            .positions
            .iter()
            .map(|p| {
                let (market_name, outcome_name) = display_names(&data.metadata, &p.token_id);
                vec![
                    market_name,
                    outcome_name,
                    p.side.clone(),
                    p.size.clone(),
                    p.avg_price.clone(),
                    p.current_price.clone(),
                    p.value.clone(),
                    p.unrealized_pnl.clone(),
                ]
            })
            .collect();
        print_output(false, headers, rows, report);
    }

    Ok(())
}

#[derive(Serialize)]
struct ResetResult {
    balance: String,
    message: String,
}

#[derive(Serialize)]
struct SummaryReport {
    starting_balance: String,
    current_balance: String,
    realized_pnl: String,
    unrealized_pnl: String,
    net_pnl: String,
    closed_trades: usize,
    wins: usize,
    losses: usize,
    win_rate: String,
    position_value: String,
    total_value: String,
}

pub async fn summary<S: State>(client: &Client<S>, json: bool) -> Result<()> {
    let data = build_pnl_data(client).await?;
    let report = &data.report;
    let realized = portfolio::compute_realized_pnl(&data.all_trades)?;

    let unrealized = Decimal::from_str(&report.total_unrealized_pnl)?;
    let net_pnl = realized.total_realized_pnl + unrealized;
    let total_closed = realized.closed_trades;
    let win_rate = if total_closed > 0 {
        format!(
            "{}/{} ({:.0}%)",
            realized.wins,
            total_closed,
            (realized.wins as f64 / total_closed as f64) * 100.0
        )
    } else {
        "N/A".to_string()
    };

    let summary = SummaryReport {
        starting_balance: report.starting_balance.clone(),
        current_balance: report.current_balance.clone(),
        realized_pnl: realized.total_realized_pnl.to_string(),
        unrealized_pnl: report.total_unrealized_pnl.clone(),
        net_pnl: net_pnl.to_string(),
        closed_trades: total_closed,
        wins: realized.wins,
        losses: realized.losses,
        win_rate: win_rate.clone(),
        position_value: report.position_value.clone(),
        total_value: report.total_value.clone(),
    };

    if json {
        print_output(true, &[], vec![], &summary);
    } else {
        println!("Realized P&L:   ${}", realized.total_realized_pnl);
        println!("Unrealized P&L: ${}", report.total_unrealized_pnl);
        println!("Net P&L:        ${net_pnl}");
        println!();
        println!("Closed Trades:  {total_closed}");
        println!("Win Rate:       {win_rate}");
        println!();
        println!("Cash:           ${}", report.current_balance);
        println!("Position Value: ${}", report.position_value);
        println!("Total Value:    ${}", report.total_value);
    }

    Ok(())
}

pub async fn portfolio<S: State>(
    client: &Client<S>,
    take_profit: Option<f64>,
    stop_loss: Option<f64>,
    json: bool,
) -> Result<()> {
    let data = build_pnl_data(client).await?;
    let report = &data.report;

    if json {
        print_output(true, &[], vec![], report);
    } else {
        println!("Balance: ${}", report.current_balance);
        println!();

        let show_status = take_profit.is_some() || stop_loss.is_some();
        let tp = take_profit.unwrap_or(15.0);
        let sl = stop_loss.unwrap_or(20.0);

        if !report.positions.is_empty() {
            let mut headers: Vec<&str> = vec![
                "#",
                "Market",
                "Outcome",
                "Side",
                "Size",
                "Avg Price",
                "Current",
                "Value",
                "P&L",
            ];
            if show_status {
                headers.push("Status");
            }
            let rows: Vec<Vec<String>> = report
                .positions
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let (market_name, outcome_name) = display_names(&data.metadata, &p.token_id);
                    let mut row = vec![
                        (i + 1).to_string(),
                        market_name,
                        outcome_name,
                        p.side.clone(),
                        p.size.clone(),
                        p.avg_price.clone(),
                        p.current_price.clone(),
                        format!("${}", p.value),
                        format!("${}", p.unrealized_pnl),
                    ];
                    if show_status {
                        let avg = Decimal::from_str(&p.avg_price).unwrap_or(Decimal::ZERO);
                        let current = Decimal::from_str(&p.current_price).unwrap_or(Decimal::ZERO);
                        let status = check_alert_status(avg, current, tp, sl);
                        let label = match status {
                            AlertStatus::TakeProfitBreached => "TP!",
                            AlertStatus::ApproachingTakeProfit => "~TP",
                            AlertStatus::StopLossBreached => "SL!",
                            AlertStatus::ApproachingStopLoss => "~SL",
                            AlertStatus::Normal => "ok",
                        };
                        row.push(label.to_string());
                    }
                    row
                })
                .collect();
            print_output(false, &headers, rows, &report.positions);
            println!();
        }

        println!("Cash:            ${}", report.current_balance);
        println!("Position Value:  ${}", report.position_value);
        println!("Total Value:     ${}", report.total_value);
        println!("Net P&L:         ${}", report.total_pnl);

        let realized = portfolio::compute_realized_pnl(&data.all_trades)?;
        if realized.total_realized_pnl != Decimal::ZERO {
            println!("Realized P&L:    ${}", realized.total_realized_pnl);
        }
    }

    Ok(())
}

#[derive(Debug, PartialEq)]
enum AlertStatus {
    Normal,
    ApproachingTakeProfit,
    TakeProfitBreached,
    ApproachingStopLoss,
    StopLossBreached,
}

fn check_alert_status(
    avg_price: Decimal,
    current_price: Decimal,
    take_profit_pct: f64,
    stop_loss_pct: f64,
) -> AlertStatus {
    if avg_price.is_zero() {
        return AlertStatus::Normal;
    }

    use rust_decimal::prelude::ToPrimitive;
    let pnl_pct = ((current_price - avg_price) / avg_price * Decimal::from(100))
        .to_f64()
        .unwrap_or(0.0);

    if pnl_pct >= take_profit_pct {
        AlertStatus::TakeProfitBreached
    } else if pnl_pct >= take_profit_pct * 0.8 {
        AlertStatus::ApproachingTakeProfit
    } else if pnl_pct <= -stop_loss_pct {
        AlertStatus::StopLossBreached
    } else if pnl_pct <= -stop_loss_pct * 0.8 {
        AlertStatus::ApproachingStopLoss
    } else {
        AlertStatus::Normal
    }
}

pub async fn alerts<S: State>(
    client: &Client<S>,
    take_profit: f64,
    stop_loss: f64,
    interval_secs: u64,
    json: bool,
) -> Result<()> {
    use std::io::{self, Write};
    use tokio::time::{Duration, interval};

    let db = DryRunDb::open()?;
    let all_trades = db.all_trades()?;
    let positions = portfolio::compute_positions(&all_trades)?;
    let metadata = db.all_metadata()?;

    anyhow::ensure!(!positions.is_empty(), "No open positions to monitor");

    eprintln!(
        "Monitoring {} positions (TP: {take_profit}%, SL: {stop_loss}%, interval: {interval_secs}s)",
        positions.len()
    );
    eprintln!("Press Ctrl+C to stop.\n");

    let mut ticker = interval(Duration::from_secs(interval_secs));
    let num_lines = positions.len();
    let mut first_tick = true;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if !json && !first_tick {
                    eprint!("\x1B[{}A", num_lines);
                }
                first_tick = false;

                let futs: Vec<_> = positions
                    .iter()
                    .map(|pos| async move {
                        let mid = fetch_midpoint(client, &pos.token_id).await;
                        (pos, mid)
                    })
                    .collect();
                let results = futures::future::join_all(futs).await;

                for (pos, mid_result) in results {
                    let (market_name, _outcome_name) = display_names(&metadata, &pos.token_id);
                    let avg = Decimal::from_str(&pos.avg_price).unwrap_or(Decimal::ZERO);

                    match mid_result {
                        Ok(mid) => {
                            let status = check_alert_status(avg, mid, take_profit, stop_loss);
                            let pnl_pct = if !avg.is_zero() {
                                ((mid - avg) / avg * Decimal::from(100)).round_dp(1)
                            } else {
                                Decimal::ZERO
                            };

                            let indicator = match status {
                                AlertStatus::TakeProfitBreached => "!! TP BREACHED",
                                AlertStatus::ApproachingTakeProfit => " ~ approaching TP",
                                AlertStatus::StopLossBreached => "!! SL BREACHED",
                                AlertStatus::ApproachingStopLoss => " ~ approaching SL",
                                AlertStatus::Normal => "  ok",
                            };

                            if json {
                                let tick = serde_json::json!({
                                    "market": market_name,
                                    "avg_price": pos.avg_price,
                                    "current_price": mid.to_string(),
                                    "pnl_pct": pnl_pct.to_string(),
                                    "status": format!("{:?}", status),
                                });
                                println!("{}", serde_json::to_string(&tick)?);
                            } else {
                                let sign = if pnl_pct >= Decimal::ZERO { "+" } else { "" };
                                eprintln!(
                                    "  {:<40} {sign}{pnl_pct}%  {indicator}",
                                    market_name,
                                );
                            }
                        }
                        Err(e) => {
                            if !json {
                                eprintln!("  {:<40} Error: {e}", market_name);
                            }
                        }
                    }
                }

                if !json {
                    io::stderr().flush()?;
                }
            }
            _ = tokio::signal::ctrl_c() => {
                break;
            }
        }
    }

    Ok(())
}

/// Print a note when outcome was not explicitly specified, to improve discoverability.
pub fn print_outcome_note(resolved: &ResolvedMarket, outcome_was_specified: bool) {
    if !outcome_was_specified && let Some(outcome) = &resolved.outcome {
        eprintln!(
            "Note: defaulted to {outcome} outcome. Use --outcome to select a different side."
        );
    }
}

pub fn reset(balance: &str, json: bool) -> Result<()> {
    let _ = Decimal::from_str(balance).map_err(|e| anyhow::anyhow!("Invalid balance: {e}"))?;

    let db = DryRunDb::open()?;
    db.reset(balance)?;

    let result = ResetResult {
        balance: balance.to_string(),
        message: format!("Dry-run state reset with balance {balance}"),
    };

    let headers = &["Balance", "Message"];
    let rows = vec![vec![result.balance.clone(), result.message.clone()]];
    print_output(json, headers, rows, &result);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dry_run::db::DryRunDb;
    use polymarket_client_sdk::types::U256;

    fn dec(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    fn test_resolved(token_id: &str) -> ResolvedMarket {
        ResolvedMarket {
            token_id: U256::ZERO,
            token_id_str: token_id.to_string(),
            slug: None,
            question: None,
            outcome: None,
        }
    }

    #[test]
    fn record_trade_buy_deducts_balance() {
        let db = DryRunDb::open_in_memory().unwrap();
        let resolved = test_resolved("tok_a");
        // Default balance is 1000.00
        record_trade(
            &db,
            &resolved,
            Side::Buy,
            dec("100.00"),
            dec("200"),
            dec("0.50"),
            false,
        )
        .unwrap();

        assert_eq!(db.get_balance().unwrap(), "900.00");
        let trades = db.all_trades().unwrap();
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].side, "buy");
        assert_eq!(trades[0].token_id, "tok_a");
    }

    #[test]
    fn record_trade_buy_insufficient_balance() {
        let db = DryRunDb::open_in_memory().unwrap();
        let resolved = test_resolved("tok_a");
        let result = record_trade(
            &db,
            &resolved,
            Side::Buy,
            dec("2000.00"),
            dec("100"),
            dec("20.0"),
            false,
        );

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Insufficient balance"));
        // Balance should be unchanged
        assert_eq!(db.get_balance().unwrap(), "1000.00");
        // No trade should be recorded
        assert_eq!(db.all_trades().unwrap().len(), 0);
    }

    #[test]
    fn record_trade_sell_credits_balance() {
        let db = DryRunDb::open_in_memory().unwrap();
        let resolved = test_resolved("tok_a");
        // First buy some shares
        record_trade(
            &db,
            &resolved,
            Side::Buy,
            dec("50.00"),
            dec("100"),
            dec("0.50"),
            false,
        )
        .unwrap();
        assert_eq!(db.get_balance().unwrap(), "950.00");

        // Now sell some
        record_trade(
            &db,
            &resolved,
            Side::Sell,
            dec("30.00"),
            dec("50"),
            dec("0.60"),
            false,
        )
        .unwrap();
        assert_eq!(db.get_balance().unwrap(), "980.00");
    }

    #[test]
    fn record_trade_sell_insufficient_position() {
        let db = DryRunDb::open_in_memory().unwrap();
        let resolved = test_resolved("tok_a");
        // No position at all
        let result = record_trade(
            &db,
            &resolved,
            Side::Sell,
            dec("50.00"),
            dec("10"),
            dec("5.0"),
            false,
        );

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Insufficient position"));
    }

    #[test]
    fn record_trade_sell_more_than_held() {
        let db = DryRunDb::open_in_memory().unwrap();
        let resolved = test_resolved("tok_a");
        // Buy 5 shares
        record_trade(
            &db,
            &resolved,
            Side::Buy,
            dec("25.00"),
            dec("5"),
            dec("5.0"),
            false,
        )
        .unwrap();

        // Try to sell 10
        let result = record_trade(
            &db,
            &resolved,
            Side::Sell,
            dec("50.00"),
            dec("10"),
            dec("5.0"),
            false,
        );

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Insufficient position")
        );
    }

    #[test]
    fn record_trade_buy_exact_balance() {
        let db = DryRunDb::open_in_memory().unwrap();
        let resolved = test_resolved("tok_a");
        // Spend exactly 1000.00
        record_trade(
            &db,
            &resolved,
            Side::Buy,
            dec("1000.00"),
            dec("2000"),
            dec("0.50"),
            false,
        )
        .unwrap();

        assert_eq!(db.get_balance().unwrap(), "0.00");
    }

    #[test]
    fn record_trade_multiple_tokens() {
        let db = DryRunDb::open_in_memory().unwrap();
        let resolved_a = test_resolved("tok_a");
        let resolved_b = test_resolved("tok_b");
        record_trade(
            &db,
            &resolved_a,
            Side::Buy,
            dec("100.00"),
            dec("10"),
            dec("10.0"),
            false,
        )
        .unwrap();
        record_trade(
            &db,
            &resolved_b,
            Side::Buy,
            dec("200.00"),
            dec("20"),
            dec("10.0"),
            false,
        )
        .unwrap();

        assert_eq!(db.get_balance().unwrap(), "700.00");
        assert_eq!(db.all_trades().unwrap().len(), 2);

        // Selling tok_a should not be affected by tok_b position
        let result = record_trade(
            &db,
            &resolved_a,
            Side::Sell,
            dec("50.00"),
            dec("15"),
            dec("10.0"),
            false,
        );
        assert!(result.is_err()); // only 10 held for tok_a
    }

    #[test]
    fn close_full_position_zeros_it() {
        let db = DryRunDb::open_in_memory().unwrap();
        let resolved = test_resolved("tok_a");
        // Buy 10 shares
        record_trade(
            &db,
            &resolved,
            Side::Buy,
            dec("50.00"),
            dec("10"),
            dec("5.0"),
            false,
        )
        .unwrap();
        assert_eq!(db.get_balance().unwrap(), "950.00");

        // Sell all 10 shares at 6.0 midpoint
        record_trade(
            &db,
            &resolved,
            Side::Sell,
            dec("60.00"),
            dec("10"),
            dec("6.0"),
            false,
        )
        .unwrap();
        assert_eq!(db.get_balance().unwrap(), "1010.00");

        // Position should be zero
        assert_eq!(db.net_position_size("tok_a").unwrap(), Decimal::ZERO);
    }

    #[test]
    fn close_partial_position() {
        let db = DryRunDb::open_in_memory().unwrap();
        let resolved = test_resolved("tok_a");
        record_trade(
            &db,
            &resolved,
            Side::Buy,
            dec("50.00"),
            dec("10"),
            dec("5.0"),
            false,
        )
        .unwrap();

        // Sell 4 shares
        record_trade(
            &db,
            &resolved,
            Side::Sell,
            dec("24.00"),
            dec("4"),
            dec("6.0"),
            false,
        )
        .unwrap();
        assert_eq!(db.net_position_size("tok_a").unwrap(), dec("6"));
    }

    #[test]
    fn alert_status_no_alert() {
        let status = check_alert_status(dec("0.50"), dec("0.525"), 15.0, 20.0);
        assert_eq!(status, AlertStatus::Normal);
    }

    #[test]
    fn alert_status_approaching_take_profit() {
        // avg 0.50, current 0.56 → 12% gain, TP at 15% → approaching (>80% of 15 = 12)
        let status = check_alert_status(dec("0.50"), dec("0.56"), 15.0, 20.0);
        assert_eq!(status, AlertStatus::ApproachingTakeProfit);
    }

    #[test]
    fn alert_status_breached_take_profit() {
        // avg 0.50, current 0.60 → 20% gain, TP at 15% → breached
        let status = check_alert_status(dec("0.50"), dec("0.60"), 15.0, 20.0);
        assert_eq!(status, AlertStatus::TakeProfitBreached);
    }

    #[test]
    fn alert_status_approaching_stop_loss() {
        // avg 0.50, current 0.42 → -16% loss, SL at 20%, 80% of 20 = 16 → approaching
        let status = check_alert_status(dec("0.50"), dec("0.42"), 15.0, 20.0);
        assert_eq!(status, AlertStatus::ApproachingStopLoss);
    }

    #[test]
    fn alert_status_breached_stop_loss() {
        // avg 0.50, current 0.38 → -24% loss, SL at 20% → breached
        let status = check_alert_status(dec("0.50"), dec("0.38"), 15.0, 20.0);
        assert_eq!(status, AlertStatus::StopLossBreached);
    }

    #[test]
    fn resolve_position_by_index() {
        let db = DryRunDb::open_in_memory().unwrap();
        let resolved_a = test_resolved("tok_a");
        let resolved_b = test_resolved("tok_b");
        record_trade(
            &db,
            &resolved_a,
            Side::Buy,
            dec("50.00"),
            dec("10"),
            dec("0.50"),
            false,
        )
        .unwrap();
        record_trade(
            &db,
            &resolved_b,
            Side::Buy,
            dec("30.00"),
            dec("10"),
            dec("0.30"),
            false,
        )
        .unwrap();

        let trades = db.all_trades().unwrap();
        let positions = portfolio::compute_positions(&trades).unwrap();
        assert_eq!(positions.len(), 2);
        assert_eq!(positions[0].token_id, "tok_a");
        assert_eq!(positions[1].token_id, "tok_b");
    }
}
