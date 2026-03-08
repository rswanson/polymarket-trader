use anyhow::{Context, Result};
use polymarket_client_sdk::auth::state::State;
use polymarket_client_sdk::clob::Client;
use polymarket_client_sdk::clob::types::request::{
    MidpointRequest, OrderBookSummaryRequest, SpreadRequest,
};
use polymarket_client_sdk::types::U256;
use serde::Serialize;
use std::str::FromStr;

use crate::output::print_output;

#[derive(Serialize)]
struct MidpointResult {
    token_id: String,
    midpoint: String,
}

#[derive(Serialize)]
struct SpreadResult {
    token_id: String,
    spread: String,
}

#[derive(Serialize)]
struct BookResult {
    token_id: String,
    bids: Vec<OrderLevel>,
    asks: Vec<OrderLevel>,
}

#[derive(Serialize)]
struct OrderLevel {
    price: String,
    size: String,
}

pub async fn midpoint<S: State>(client: &Client<S>, token_id_str: &str, json: bool) -> Result<()> {
    let token_id =
        U256::from_str(token_id_str).map_err(|e| anyhow::anyhow!("Invalid token ID: {e}"))?;

    let request = MidpointRequest::builder().token_id(token_id).build();
    let response = client
        .midpoint(&request)
        .await
        .context("Failed to fetch midpoint")?;

    let result = MidpointResult {
        token_id: token_id_str.to_string(),
        midpoint: response.mid.to_string(),
    };

    let headers = &["Token ID", "Midpoint"];
    let rows = vec![vec![result.token_id.clone(), result.midpoint.clone()]];
    print_output(json, headers, rows, &result);

    Ok(())
}

pub async fn spread<S: State>(client: &Client<S>, token_id_str: &str, json: bool) -> Result<()> {
    let token_id =
        U256::from_str(token_id_str).map_err(|e| anyhow::anyhow!("Invalid token ID: {e}"))?;

    let request = SpreadRequest::builder().token_id(token_id).build();
    let response = client
        .spread(&request)
        .await
        .context("Failed to fetch spread")?;

    let result = SpreadResult {
        token_id: token_id_str.to_string(),
        spread: response.spread.to_string(),
    };

    let headers = &["Token ID", "Spread"];
    let rows = vec![vec![result.token_id.clone(), result.spread.clone()]];
    print_output(json, headers, rows, &result);

    Ok(())
}

pub async fn book<S: State>(client: &Client<S>, token_id_str: &str, json: bool) -> Result<()> {
    let token_id =
        U256::from_str(token_id_str).map_err(|e| anyhow::anyhow!("Invalid token ID: {e}"))?;

    let request = OrderBookSummaryRequest::builder()
        .token_id(token_id)
        .build();
    let response = client
        .order_book(&request)
        .await
        .context("Failed to fetch order book")?;

    let result = BookResult {
        token_id: token_id_str.to_string(),
        bids: response
            .bids
            .iter()
            .map(|o| OrderLevel {
                price: o.price.to_string(),
                size: o.size.to_string(),
            })
            .collect(),
        asks: response
            .asks
            .iter()
            .map(|o| OrderLevel {
                price: o.price.to_string(),
                size: o.size.to_string(),
            })
            .collect(),
    };

    let headers = &["Side", "Price", "Size"];
    let mut rows = Vec::new();
    for bid in &result.bids {
        rows.push(vec!["BID".to_string(), bid.price.clone(), bid.size.clone()]);
    }
    for ask in &result.asks {
        rows.push(vec!["ASK".to_string(), ask.price.clone(), ask.size.clone()]);
    }
    print_output(json, headers, rows, &result);

    Ok(())
}
