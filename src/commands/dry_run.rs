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
use crate::dry_run::db::{DryRunDb, Trade};
use crate::dry_run::portfolio;
use crate::output::print_output;
use crate::resolve::ResolvedMarket;

fn truncate_token_id(token_id: &str) -> String {
    if token_id.len() > 12 {
        token_id.chars().take(12).collect::<String>() + "..."
    } else {
        token_id.to_string()
    }
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() > max_len {
        let end = s
            .char_indices()
            .nth(max_len.saturating_sub(3))
            .map(|(i, _)| i)
            .unwrap_or(s.len());
        format!("{}...", &s[..end])
    } else {
        s.to_string()
    }
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
#[allow(clippy::too_many_arguments)]
fn record_trade(
    db: &DryRunDb,
    token_id: &str,
    side: Side,
    cost: Decimal,
    size: Decimal,
    midpoint: Decimal,
    json: bool,
    slug: Option<&str>,
    question: Option<&str>,
    outcome: Option<&str>,
) -> Result<()> {
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
        let held = db.net_position_size(token_id)?;
        let held_dec = Decimal::from_f64_retain(held).unwrap_or_default();
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
    db.upsert_metadata(token_id, slug, question, outcome)?;
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
    record_trade(
        &db,
        &resolved.token_id_str,
        side,
        cost,
        size,
        midpoint,
        json,
        resolved.slug.as_deref(),
        resolved.question.as_deref(),
        resolved.outcome.as_deref(),
    )
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
    record_trade(
        &db,
        &resolved.token_id_str,
        side,
        cost,
        size,
        midpoint,
        json,
        resolved.slug.as_deref(),
        resolved.question.as_deref(),
        resolved.outcome.as_deref(),
    )
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
        "Market",
        "Outcome",
        "Side",
        "Size",
        "Avg Price",
        "Total Cost",
    ];
    let rows: Vec<Vec<String>> = positions
        .iter()
        .map(|p| {
            let meta = metadata.get(&p.token_id);
            let market_name = meta
                .and_then(|m| m.question.as_deref())
                .map(|q| truncate_str(q, 40))
                .unwrap_or_else(|| truncate_token_id(&p.token_id));
            let outcome_name = meta
                .and_then(|m| m.outcome.as_deref())
                .unwrap_or("-")
                .to_string();
            vec![
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
            let meta = metadata.get(&t.token_id);
            let market_name = meta
                .and_then(|m| m.question.as_deref())
                .map(|q| truncate_str(q, 40))
                .unwrap_or_else(|| truncate_token_id(&t.token_id));
            let outcome_name = meta
                .and_then(|m| m.outcome.as_deref())
                .unwrap_or("-")
                .to_string();
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
    let db = DryRunDb::open()?;
    let all_trades = db.all_trades()?;
    let positions = portfolio::compute_positions(&all_trades)?;
    let metadata = db.all_metadata()?;

    // Fetch all midpoint prices concurrently
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

    if json {
        print_output(true, &[], vec![], &report);
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
                let meta = metadata.get(&p.token_id);
                let market_name = meta
                    .and_then(|m| m.question.as_deref())
                    .map(|q| truncate_str(q, 40))
                    .unwrap_or_else(|| truncate_token_id(&p.token_id));
                let outcome_name = meta
                    .and_then(|m| m.outcome.as_deref())
                    .unwrap_or("-")
                    .to_string();
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
        print_output(false, headers, rows, &report);
    }

    Ok(())
}

#[derive(Serialize)]
struct ResetResult {
    balance: String,
    message: String,
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

    fn dec(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn record_trade_buy_deducts_balance() {
        let db = DryRunDb::open_in_memory().unwrap();
        // Default balance is 1000.00
        record_trade(
            &db,
            "tok_a",
            Side::Buy,
            dec("100.00"),
            dec("200"),
            dec("0.50"),
            false,
            None,
            None,
            None,
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
        let result = record_trade(
            &db,
            "tok_a",
            Side::Buy,
            dec("2000.00"),
            dec("100"),
            dec("20.0"),
            false,
            None,
            None,
            None,
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
        // First buy some shares
        record_trade(
            &db,
            "tok_a",
            Side::Buy,
            dec("50.00"),
            dec("100"),
            dec("0.50"),
            false,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(db.get_balance().unwrap(), "950.00");

        // Now sell some
        record_trade(
            &db,
            "tok_a",
            Side::Sell,
            dec("30.00"),
            dec("50"),
            dec("0.60"),
            false,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(db.get_balance().unwrap(), "980.00");
    }

    #[test]
    fn record_trade_sell_insufficient_position() {
        let db = DryRunDb::open_in_memory().unwrap();
        // No position at all
        let result = record_trade(
            &db,
            "tok_a",
            Side::Sell,
            dec("50.00"),
            dec("10"),
            dec("5.0"),
            false,
            None,
            None,
            None,
        );

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Insufficient position"));
    }

    #[test]
    fn record_trade_sell_more_than_held() {
        let db = DryRunDb::open_in_memory().unwrap();
        // Buy 5 shares
        record_trade(
            &db,
            "tok_a",
            Side::Buy,
            dec("25.00"),
            dec("5"),
            dec("5.0"),
            false,
            None,
            None,
            None,
        )
        .unwrap();

        // Try to sell 10
        let result = record_trade(
            &db,
            "tok_a",
            Side::Sell,
            dec("50.00"),
            dec("10"),
            dec("5.0"),
            false,
            None,
            None,
            None,
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
        // Spend exactly 1000.00
        record_trade(
            &db,
            "tok_a",
            Side::Buy,
            dec("1000.00"),
            dec("2000"),
            dec("0.50"),
            false,
            None,
            None,
            None,
        )
        .unwrap();

        assert_eq!(db.get_balance().unwrap(), "0.00");
    }

    #[test]
    fn record_trade_multiple_tokens() {
        let db = DryRunDb::open_in_memory().unwrap();
        record_trade(
            &db,
            "tok_a",
            Side::Buy,
            dec("100.00"),
            dec("10"),
            dec("10.0"),
            false,
            None,
            None,
            None,
        )
        .unwrap();
        record_trade(
            &db,
            "tok_b",
            Side::Buy,
            dec("200.00"),
            dec("20"),
            dec("10.0"),
            false,
            None,
            None,
            None,
        )
        .unwrap();

        assert_eq!(db.get_balance().unwrap(), "700.00");
        assert_eq!(db.all_trades().unwrap().len(), 2);

        // Selling tok_a should not be affected by tok_b position
        let result = record_trade(
            &db,
            "tok_a",
            Side::Sell,
            dec("50.00"),
            dec("15"),
            dec("10.0"),
            false,
            None,
            None,
            None,
        );
        assert!(result.is_err()); // only 10 held for tok_a
    }
}
