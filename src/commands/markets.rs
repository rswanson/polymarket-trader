use anyhow::{Context, Result};
use polymarket_client_sdk::auth::state::State;
use polymarket_client_sdk::clob::Client;
use serde::Serialize;

use crate::output::print_output;

#[derive(Serialize)]
struct MarketRow {
    condition_id: String,
    active: bool,
    tokens: Vec<TokenInfo>,
}

#[derive(Serialize)]
struct TokenInfo {
    token_id: String,
    outcome: String,
    price: String,
}

#[derive(Serialize)]
struct MarketDetail {
    condition_id: String,
    question: String,
    active: bool,
    closed: bool,
    neg_risk: bool,
    tokens: Vec<TokenInfo>,
}

pub async fn list_markets<S: State>(client: &Client<S>, limit: usize, json: bool) -> Result<()> {
    let mut all_markets = Vec::new();
    let mut cursor = None;

    loop {
        let page = client
            .simplified_markets(cursor)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        for m in &page.data {
            all_markets.push(MarketRow {
                condition_id: m.condition_id.map(|c| format!("{c}")).unwrap_or_default(),
                active: m.active,
                tokens: m
                    .tokens
                    .iter()
                    .map(|t| TokenInfo {
                        token_id: t.token_id.to_string(),
                        outcome: t.outcome.clone(),
                        price: t.price.to_string(),
                    })
                    .collect(),
            });

            if all_markets.len() >= limit {
                break;
            }
        }

        if all_markets.len() >= limit || page.next_cursor == "LTE=" || page.data.is_empty() {
            break;
        }

        cursor = Some(page.next_cursor);
    }

    all_markets.truncate(limit);

    let headers = &["Condition ID", "Active", "Tokens"];
    let rows: Vec<Vec<String>> = all_markets
        .iter()
        .map(|m| {
            let tokens_str: Vec<String> = m
                .tokens
                .iter()
                .map(|t| format!("{}: {}", t.outcome, t.price))
                .collect();
            vec![
                m.condition_id.clone(),
                m.active.to_string(),
                tokens_str.join(", "),
            ]
        })
        .collect();

    print_output(json, headers, rows, &all_markets);

    Ok(())
}

pub async fn show_market<S: State>(
    client: &Client<S>,
    condition_id: &str,
    json: bool,
) -> Result<()> {
    let market = client
        .market(condition_id)
        .await
        .context("Failed to fetch market")?;

    let detail = MarketDetail {
        condition_id: market
            .condition_id
            .map(|c| format!("{c}"))
            .unwrap_or_default(),
        question: market.question.clone(),
        active: market.active,
        closed: market.closed,
        neg_risk: market.neg_risk,
        tokens: market
            .tokens
            .iter()
            .map(|t| TokenInfo {
                token_id: t.token_id.to_string(),
                outcome: t.outcome.clone(),
                price: t.price.to_string(),
            })
            .collect(),
    };

    let headers = &[
        "Condition ID",
        "Question",
        "Active",
        "Closed",
        "Neg Risk",
        "Tokens",
    ];
    let tokens_str: Vec<String> = detail
        .tokens
        .iter()
        .map(|t| format!("{} ({}): {}", t.outcome, t.token_id, t.price))
        .collect();
    let rows = vec![vec![
        detail.condition_id.clone(),
        detail.question.clone(),
        detail.active.to_string(),
        detail.closed.to_string(),
        detail.neg_risk.to_string(),
        tokens_str.join("\n"),
    ]];

    print_output(json, headers, rows, &detail);

    Ok(())
}
