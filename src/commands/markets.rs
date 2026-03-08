use anyhow::{Context, Result};
use polymarket_client_sdk::auth::state::State;
use polymarket_client_sdk::clob::Client as ClobClient;
use polymarket_client_sdk::gamma;
use polymarket_client_sdk::gamma::types::request::{
    MarketBySlugRequest, MarketsRequest, SearchRequest,
};
use polymarket_client_sdk::gamma::types::response::Market;
use polymarket_client_sdk::types::Decimal;
use serde::Serialize;
use std::str::FromStr;

use crate::output::{print_output, truncate};

#[derive(Serialize)]
struct GammaMarketRow {
    slug: String,
    question: String,
    volume: String,
    outcomes: String,
}

#[derive(Serialize)]
struct MarketDetail {
    slug: String,
    question: String,
    active: bool,
    closed: bool,
    volume: String,
    tokens: Vec<TokenInfo>,
}

#[derive(Serialize)]
struct TokenInfo {
    token_id: String,
    outcome: String,
    price: String,
}

fn format_outcomes(m: &Market) -> String {
    let outcomes = m.outcomes.as_ref();
    let prices = m.outcome_prices.as_ref();
    match (outcomes, prices) {
        (Some(names), Some(vals)) => names
            .iter()
            .zip(vals.iter())
            .map(|(name, price)| format!("{name}: {price}"))
            .collect::<Vec<_>>()
            .join(", "),
        (Some(names), None) => names.join(", "),
        _ => String::new(),
    }
}

fn market_to_row(m: &Market) -> GammaMarketRow {
    GammaMarketRow {
        slug: m.slug.clone().unwrap_or_default(),
        question: m.question.clone().unwrap_or_default(),
        volume: m.volume.map(|v| v.to_string()).unwrap_or_default(),
        outcomes: format_outcomes(m),
    }
}

pub async fn list_markets(
    gamma_client: &gamma::Client,
    limit: usize,
    query: Option<&str>,
    include_closed: bool,
    sort: &str,
    min_volume: Option<&str>,
    json: bool,
) -> Result<()> {
    let markets: Vec<Market> = if let Some(q) = query {
        let request = SearchRequest::builder().q(q).build();
        let results = gamma_client
            .search(&request)
            .await
            .context("Failed to search markets")?;
        results
            .events
            .unwrap_or_default()
            .into_iter()
            .flat_map(|event| event.markets.unwrap_or_default())
            .collect()
    } else {
        let vol_min = match min_volume {
            Some(v) => Some(Decimal::from_str(v).context("Invalid min_volume value")?),
            None => None,
        };

        let mut request = MarketsRequest::default();
        request.limit = Some(limit as i32);
        request.order = Some(sort.to_string());
        request.ascending = Some(false);
        request.volume_num_min = vol_min;
        if !include_closed {
            request.closed = Some(false);
        }

        gamma_client
            .markets(&request)
            .await
            .context("Failed to fetch markets")?
    };

    let markets: Vec<Market> = markets.into_iter().take(limit).collect();
    let rows_data: Vec<GammaMarketRow> = markets.iter().map(market_to_row).collect();

    let headers = &["Slug", "Question", "Volume", "Outcomes"];
    let table_rows: Vec<Vec<String>> = rows_data
        .iter()
        .map(|r| {
            vec![
                r.slug.clone(),
                truncate(&r.question, 60),
                r.volume.clone(),
                r.outcomes.clone(),
            ]
        })
        .collect();

    print_output(json, headers, table_rows, &rows_data);

    Ok(())
}

pub async fn show_market<S: State>(
    gamma_client: &gamma::Client,
    clob_client: &ClobClient<S>,
    market: &str,
    json: bool,
) -> Result<()> {
    let gamma_result = gamma_client
        .market_by_slug(&MarketBySlugRequest::builder().slug(market).build())
        .await;

    let detail = match gamma_result {
        Ok(m) => {
            let tokens = build_tokens_from_gamma(&m);
            MarketDetail {
                slug: m.slug.clone().unwrap_or_default(),
                question: m.question.clone().unwrap_or_default(),
                active: m.active.unwrap_or(false),
                closed: m.closed.unwrap_or(false),
                volume: m.volume.map(|v| v.to_string()).unwrap_or_default(),
                tokens,
            }
        }
        Err(e) => {
            tracing::debug!("Gamma slug lookup failed, falling back to CLOB: {e}");
            // Fall back to CLOB by condition_id
            let m = clob_client
                .market(market)
                .await
                .context("Failed to fetch market by condition ID")?;
            MarketDetail {
                slug: String::new(),
                question: m.question.clone(),
                active: m.active,
                closed: m.closed,
                volume: String::new(),
                tokens: m
                    .tokens
                    .iter()
                    .map(|t| TokenInfo {
                        token_id: t.token_id.to_string(),
                        outcome: t.outcome.clone(),
                        price: t.price.to_string(),
                    })
                    .collect(),
            }
        }
    };

    let headers = &["Slug", "Question", "Active", "Closed", "Volume", "Tokens"];
    let tokens_str: Vec<String> = detail
        .tokens
        .iter()
        .map(|t| format!("{} ({}): {}", t.outcome, t.token_id, t.price))
        .collect();
    let rows = vec![vec![
        detail.slug.clone(),
        detail.question.clone(),
        detail.active.to_string(),
        detail.closed.to_string(),
        detail.volume.clone(),
        tokens_str.join("\n"),
    ]];

    print_output(json, headers, rows, &detail);

    Ok(())
}

fn build_tokens_from_gamma(m: &Market) -> Vec<TokenInfo> {
    let outcomes = m.outcomes.as_ref();
    let prices = m.outcome_prices.as_ref();
    let token_ids = m.clob_token_ids.as_ref();

    let outcome_names: Vec<&str> = outcomes
        .map(|o| o.iter().map(String::as_str).collect())
        .unwrap_or_default();

    let price_strs: Vec<String> = prices
        .map(|p| p.iter().map(ToString::to_string).collect())
        .unwrap_or_default();

    let id_strs: Vec<String> = token_ids
        .map(|ids| ids.iter().map(ToString::to_string).collect())
        .unwrap_or_default();

    let count = outcome_names.len();
    (0..count)
        .map(|i| TokenInfo {
            token_id: id_strs.get(i).cloned().unwrap_or_default(),
            outcome: outcome_names.get(i).unwrap_or(&"").to_string(),
            price: price_strs.get(i).cloned().unwrap_or_default(),
        })
        .collect()
}
