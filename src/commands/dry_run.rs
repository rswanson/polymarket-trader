use std::collections::HashMap;
use std::str::FromStr;

use anyhow::{Context, Result};
use chrono::Utc;
use polymarket_client_sdk::auth::state::State;
use polymarket_client_sdk::clob::Client;
use polymarket_client_sdk::clob::types::request::MidpointRequest;
use polymarket_client_sdk::types::{Decimal, U256};
use serde::Serialize;
use uuid::Uuid;

use crate::dry_run::db::{DryRunDb, Trade};
use crate::dry_run::portfolio;
use crate::output::print_output;

fn truncate_token_id(token_id: &str) -> String {
    if token_id.len() > 12 {
        token_id.chars().take(12).collect::<String>() + "..."
    } else {
        token_id.to_string()
    }
}

fn parse_side(s: &str) -> Result<String> {
    match s.to_lowercase().as_str() {
        "buy" => Ok("buy".to_string()),
        "sell" => Ok("sell".to_string()),
        other => Err(anyhow::anyhow!(
            "Invalid side '{other}', expected 'buy' or 'sell'"
        )),
    }
}

async fn fetch_midpoint<S: State>(client: &Client<S>, token_id_str: &str) -> Result<Decimal> {
    let token_id =
        U256::from_str(token_id_str).map_err(|e| anyhow::anyhow!("Invalid token ID: {e}"))?;
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

pub async fn place_limit<S: State>(
    client: &Client<S>,
    token_id: &str,
    side_str: &str,
    _price_str: &str,
    size_str: &str,
    json: bool,
) -> Result<()> {
    let side = parse_side(side_str)?;
    let size = Decimal::from_str(size_str).map_err(|e| anyhow::anyhow!("Invalid size: {e}"))?;
    let midpoint = fetch_midpoint(client, token_id).await?;
    let cost = (midpoint * size).round_dp(2);

    let db = DryRunDb::open()?;
    let mut balance =
        Decimal::from_str(&db.get_balance()?).context("invalid balance in database")?;

    if side == "buy" {
        anyhow::ensure!(
            balance >= cost,
            "Insufficient balance: have {balance}, need {cost}"
        );
        balance -= cost;
    } else {
        let all_trades = db.all_trades()?;
        let positions = portfolio::compute_positions(&all_trades)?;
        let held = positions
            .iter()
            .find(|p| p.token_id == token_id && p.side == "long")
            .map(|p| Decimal::from_str(&p.net_size).unwrap_or_default())
            .unwrap_or_default();
        anyhow::ensure!(
            held >= size,
            "Insufficient position: hold {held} shares, trying to sell {size}"
        );
        balance += cost;
    }

    let balance = balance.round_dp(2);

    let trade = Trade {
        id: Uuid::new_v4().to_string(),
        token_id: token_id.to_string(),
        side,
        price: midpoint.round_dp(6).to_string(),
        size: size.to_string(),
        cost: cost.to_string(),
        timestamp: Utc::now().to_rfc3339(),
    };

    db.insert_trade(&trade)?;
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

pub async fn place_market<S: State>(
    client: &Client<S>,
    token_id: &str,
    side_str: &str,
    amount_str: &str,
    json: bool,
) -> Result<()> {
    let side = parse_side(side_str)?;
    let amount =
        Decimal::from_str(amount_str).map_err(|e| anyhow::anyhow!("Invalid amount: {e}"))?;
    let midpoint = fetch_midpoint(client, token_id).await?;
    anyhow::ensure!(
        !midpoint.is_zero(),
        "Midpoint is zero for token {token_id}, cannot calculate size"
    );
    let size = amount / midpoint;
    let cost = amount.round_dp(2);

    let db = DryRunDb::open()?;
    let mut balance =
        Decimal::from_str(&db.get_balance()?).context("invalid balance in database")?;

    if side == "buy" {
        anyhow::ensure!(
            balance >= cost,
            "Insufficient balance: have {balance}, need {cost}"
        );
        balance -= cost;
    } else {
        let all_trades = db.all_trades()?;
        let positions = portfolio::compute_positions(&all_trades)?;
        let held = positions
            .iter()
            .find(|p| p.token_id == token_id && p.side == "long")
            .map(|p| Decimal::from_str(&p.net_size).unwrap_or_default())
            .unwrap_or_default();
        anyhow::ensure!(
            held >= size,
            "Insufficient position: hold {held} shares, trying to sell {size}"
        );
        balance += cost;
    }

    let balance = balance.round_dp(2);

    let trade = Trade {
        id: Uuid::new_v4().to_string(),
        token_id: token_id.to_string(),
        side,
        price: midpoint.round_dp(6).to_string(),
        size: size.to_string(),
        cost: cost.to_string(),
        timestamp: Utc::now().to_rfc3339(),
    };

    db.insert_trade(&trade)?;
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

    let headers = &["Token ID", "Side", "Size", "Avg Price", "Total Cost"];
    let rows: Vec<Vec<String>> = positions
        .iter()
        .map(|p| {
            vec![
                truncate_token_id(&p.token_id),
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

    let headers = &["ID", "Token", "Side", "Price", "Size", "Cost", "Time"];
    let rows: Vec<Vec<String>> = trades
        .iter()
        .map(|t| {
            vec![
                t.id.clone(),
                truncate_token_id(&t.token_id),
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

    let mut current_prices: HashMap<String, Decimal> = HashMap::new();
    for pos in &positions {
        let mid = fetch_midpoint(client, &pos.token_id).await?;
        current_prices.insert(pos.token_id.clone(), mid);
    }

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
        println!("Starting balance: {}", report.starting_balance);
        println!("Current balance:  {}", report.current_balance);
        println!("Unrealized P&L:   {}", report.total_unrealized_pnl);
        println!("Total P&L:        {}", report.total_pnl);
        println!();

        let headers = &[
            "Token ID",
            "Side",
            "Size",
            "Avg Price",
            "Current Price",
            "Unrealized P&L",
        ];
        let rows: Vec<Vec<String>> = report
            .positions
            .iter()
            .map(|p| {
                vec![
                    truncate_token_id(&p.token_id),
                    p.side.clone(),
                    p.size.clone(),
                    p.avg_price.clone(),
                    p.current_price.clone(),
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
