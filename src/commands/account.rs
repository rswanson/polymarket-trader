use anyhow::Result;
use polymarket_client_sdk::auth::Normal;
use polymarket_client_sdk::auth::state::Authenticated;
use polymarket_client_sdk::clob::Client;
use polymarket_client_sdk::clob::types::request::{BalanceAllowanceRequest, TradesRequest};
use serde::Serialize;

use crate::output::print_output;

#[derive(Serialize)]
struct BalanceResult {
    balance: String,
    allowances: Vec<AllowanceInfo>,
}

#[derive(Serialize)]
struct AllowanceInfo {
    address: String,
    allowance: String,
}

#[derive(Serialize)]
struct TradeRow {
    id: String,
    market: String,
    side: String,
    price: String,
    size: String,
    status: String,
}

pub async fn balance(client: &Client<Authenticated<Normal>>, json: bool) -> Result<()> {
    let response = client
        .balance_allowance(BalanceAllowanceRequest::default())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to fetch balance: {e}"))?;

    let result = BalanceResult {
        balance: response.balance.to_string(),
        allowances: response
            .allowances
            .iter()
            .map(|(addr, val)| AllowanceInfo {
                address: format!("{addr}"),
                allowance: val.clone(),
            })
            .collect(),
    };

    let headers = &["Balance", "Allowances"];
    let allowances_str: Vec<String> = result
        .allowances
        .iter()
        .map(|a| format!("{}: {}", a.address, a.allowance))
        .collect();
    let rows = vec![vec![result.balance.clone(), allowances_str.join(", ")]];
    print_output(json, headers, rows, &result);

    Ok(())
}

pub async fn trades(
    client: &Client<Authenticated<Normal>>,
    limit: usize,
    json: bool,
) -> Result<()> {
    let request = TradesRequest::default();
    let mut all_trades = Vec::new();
    let mut cursor = None;

    loop {
        let page = client
            .trades(&request, cursor)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch trades: {e}"))?;

        for t in &page.data {
            all_trades.push(TradeRow {
                id: t.id.clone(),
                market: format!("{}", t.market),
                side: t.side.to_string(),
                price: t.price.to_string(),
                size: t.size.to_string(),
                status: t.status.to_string(),
            });

            if all_trades.len() >= limit {
                break;
            }
        }

        if all_trades.len() >= limit || page.next_cursor == "LTE=" || page.data.is_empty() {
            break;
        }

        cursor = Some(page.next_cursor);
    }

    all_trades.truncate(limit);

    let headers = &["ID", "Market", "Side", "Price", "Size", "Status"];
    let rows: Vec<Vec<String>> = all_trades
        .iter()
        .map(|t| {
            vec![
                t.id.clone(),
                t.market.clone(),
                t.side.clone(),
                t.price.clone(),
                t.size.clone(),
                t.status.clone(),
            ]
        })
        .collect();

    print_output(json, headers, rows, &all_trades);

    Ok(())
}
