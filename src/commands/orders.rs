use alloy::signers::Signer;
use anyhow::Result;
use polymarket_client_sdk::auth::Normal;
use polymarket_client_sdk::auth::state::Authenticated;
use polymarket_client_sdk::clob::Client;
use polymarket_client_sdk::clob::types::request::{CancelMarketOrderRequest, OrdersRequest};
use polymarket_client_sdk::clob::types::{Amount, Side};
use polymarket_client_sdk::types::{Decimal, U256};
use serde::Serialize;
use std::str::FromStr;

use crate::output::{print_error, print_output};

#[derive(Serialize)]
struct OrderRow {
    id: String,
    market: String,
    side: String,
    price: String,
    original_size: String,
    size_matched: String,
    status: String,
}

#[derive(Serialize)]
struct PostOrderResult {
    success: bool,
    order_id: String,
    error_msg: Option<String>,
}

#[derive(Serialize)]
struct CancelResult {
    canceled: Vec<String>,
    not_canceled: Vec<String>,
}

fn parse_side(s: &str) -> Result<Side> {
    match s.to_lowercase().as_str() {
        "buy" => Ok(Side::Buy),
        "sell" => Ok(Side::Sell),
        other => Err(anyhow::anyhow!(
            "Invalid side '{other}', expected 'buy' or 'sell'"
        )),
    }
}

pub async fn list_orders(
    client: &Client<Authenticated<Normal>>,
    _all: bool,
    json: bool,
) -> Result<()> {
    let request = OrdersRequest::builder().build();
    let page = client
        .orders(&request, None)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let orders: Vec<OrderRow> = page
        .data
        .iter()
        .map(|o| OrderRow {
            id: o.id.clone(),
            market: format!("{}", o.market),
            side: o.side.to_string(),
            price: o.price.to_string(),
            original_size: o.original_size.to_string(),
            size_matched: o.size_matched.to_string(),
            status: o.status.to_string(),
        })
        .collect();

    let headers = &[
        "ID",
        "Market",
        "Side",
        "Price",
        "Original Size",
        "Size Matched",
        "Status",
    ];
    let rows: Vec<Vec<String>> = orders
        .iter()
        .map(|o| {
            vec![
                o.id.clone(),
                o.market.clone(),
                o.side.clone(),
                o.price.clone(),
                o.original_size.clone(),
                o.size_matched.clone(),
                o.status.clone(),
            ]
        })
        .collect();

    print_output(json, headers, rows, &orders);

    Ok(())
}

pub async fn place_limit<S2: Signer>(
    client: &Client<Authenticated<Normal>>,
    signer: &S2,
    token_id_str: &str,
    side_str: &str,
    price_str: &str,
    size_str: &str,
    json: bool,
) -> Result<()> {
    let token_id =
        U256::from_str(token_id_str).map_err(|e| anyhow::anyhow!("Invalid token ID: {e}"))?;
    let side = parse_side(side_str)?;
    let price = Decimal::from_str(price_str).map_err(|e| anyhow::anyhow!("Invalid price: {e}"))?;
    let size = Decimal::from_str(size_str).map_err(|e| anyhow::anyhow!("Invalid size: {e}"))?;

    let signable = client
        .limit_order()
        .token_id(token_id)
        .side(side)
        .price(price)
        .size(size)
        .build()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to build limit order: {e}"))?;

    let signed = client
        .sign(signer, signable)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to sign order: {e}"))?;

    let response = client
        .post_order(signed)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to post order: {e}"))?;

    let result = PostOrderResult {
        success: response.success,
        order_id: response.order_id.clone(),
        error_msg: response.error_msg.clone(),
    };

    if !response.success {
        print_error(
            json,
            &format!(
                "Order failed: {}",
                response.error_msg.as_deref().unwrap_or("unknown error")
            ),
        );
        return Ok(());
    }

    let headers = &["Success", "Order ID"];
    let rows = vec![vec![result.success.to_string(), result.order_id.clone()]];
    print_output(json, headers, rows, &result);

    Ok(())
}

pub async fn place_market<S2: Signer>(
    client: &Client<Authenticated<Normal>>,
    signer: &S2,
    token_id_str: &str,
    side_str: &str,
    amount_str: &str,
    json: bool,
) -> Result<()> {
    let token_id =
        U256::from_str(token_id_str).map_err(|e| anyhow::anyhow!("Invalid token ID: {e}"))?;
    let side = parse_side(side_str)?;
    let amount_dec =
        Decimal::from_str(amount_str).map_err(|e| anyhow::anyhow!("Invalid amount: {e}"))?;
    let amount =
        Amount::usdc(amount_dec).map_err(|e| anyhow::anyhow!("Invalid USDC amount: {e}"))?;

    let signable = client
        .market_order()
        .token_id(token_id)
        .side(side)
        .amount(amount)
        .build()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to build market order: {e}"))?;

    let signed = client
        .sign(signer, signable)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to sign order: {e}"))?;

    let response = client
        .post_order(signed)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to post order: {e}"))?;

    let result = PostOrderResult {
        success: response.success,
        order_id: response.order_id.clone(),
        error_msg: response.error_msg.clone(),
    };

    if !response.success {
        print_error(
            json,
            &format!(
                "Order failed: {}",
                response.error_msg.as_deref().unwrap_or("unknown error")
            ),
        );
        return Ok(());
    }

    let headers = &["Success", "Order ID"];
    let rows = vec![vec![result.success.to_string(), result.order_id.clone()]];
    print_output(json, headers, rows, &result);

    Ok(())
}

pub async fn cancel_order(
    client: &Client<Authenticated<Normal>>,
    order_id: &str,
    json: bool,
) -> Result<()> {
    let response = client
        .cancel_order(order_id)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to cancel order: {e}"))?;

    let result = CancelResult {
        canceled: response.canceled.clone(),
        not_canceled: response
            .not_canceled
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect(),
    };

    let headers = &["Canceled", "Not Canceled"];
    let rows = vec![vec![
        result.canceled.join(", "),
        result.not_canceled.join(", "),
    ]];
    print_output(json, headers, rows, &result);

    Ok(())
}

pub async fn cancel_all(
    client: &Client<Authenticated<Normal>>,
    market: Option<&str>,
    json: bool,
) -> Result<()> {
    let response = match market {
        Some(market_id) => {
            let request = CancelMarketOrderRequest::builder()
                .market(
                    market_id
                        .parse()
                        .map_err(|e| anyhow::anyhow!("Invalid market ID: {e}"))?,
                )
                .build();
            client
                .cancel_market_orders(&request)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to cancel market orders: {e}"))?
        }
        None => client
            .cancel_all_orders()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to cancel all orders: {e}"))?,
    };

    let result = CancelResult {
        canceled: response.canceled.clone(),
        not_canceled: response
            .not_canceled
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect(),
    };

    let headers = &["Canceled", "Not Canceled"];
    let rows = vec![vec![
        result.canceled.join(", "),
        result.not_canceled.join(", "),
    ]];
    print_output(json, headers, rows, &result);

    Ok(())
}
