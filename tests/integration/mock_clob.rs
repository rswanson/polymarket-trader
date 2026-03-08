//! Mock CLOB HTTP server for integration tests.
//!
//! Implements the subset of Polymarket CLOB API endpoints that the
//! `polymarket-client-sdk` calls, returning canned responses suitable
//! for testing the CLI without hitting the real API.

use axum::{
    Json, Router,
    extract::State,
    routing::{delete, get, post},
};
use serde_json::{Value, json};
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

/// Shared state for the mock CLOB server.
pub struct MockClobState {
    /// The RPC URL of the backing Anvil node (for reference by tests).
    pub rpc_url: String,
    /// All order bodies received via `POST /order`.
    pub orders_received: Mutex<Vec<Value>>,
}

/// Start the mock CLOB server on a random port.
///
/// Returns the bound socket address and a handle to the shared state so tests
/// can inspect received orders.
pub async fn start_mock_clob(rpc_url: String) -> (SocketAddr, Arc<MockClobState>) {
    let state = Arc::new(MockClobState {
        rpc_url,
        orders_received: Mutex::new(Vec::new()),
    });

    let app = Router::new()
        // Server time
        .route("/time", get(handle_time))
        // Authentication
        .route("/auth/api-key", post(handle_auth))
        .route("/auth/derive-api-key", get(handle_auth))
        // Orders
        .route("/order", post(handle_post_order))
        .route("/order", delete(handle_cancel_order))
        .route("/data/orders", get(handle_get_orders))
        .route("/cancel-all", delete(handle_cancel_all))
        .route("/cancel-market-orders", delete(handle_cancel_market_orders))
        // Balance
        .route("/balance-allowance", get(handle_balance_allowance))
        // Trades
        .route("/data/trades", get(handle_get_trades))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind mock CLOB listener");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (addr, state)
}

// ── Handlers ────────────────────────────────────────────────────────────────

/// `GET /time` — return current unix timestamp as a string.
async fn handle_time() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    now.to_string()
}

/// `POST /auth/api-key` and `GET /auth/derive-api-key` — return fake API
/// credentials.  The `secret` is a valid base64-encoded string so the SDK
/// can decode it for HMAC key derivation.
async fn handle_auth() -> Json<Value> {
    // "dGVzdC1zZWNyZXQta2V5LWZvci1obWFjLTEyMzQ=" is the base64 encoding of
    // "test-secret-key-for-hmac-1234".
    Json(json!({
        "apiKey": "00000000-0000-0000-0000-000000000001",
        "secret": "dGVzdC1zZWNyZXQta2V5LWZvci1obWFjLTEyMzQ=",
        "passphrase": "test-passphrase"
    }))
}

/// `POST /order` — store the order body and return a success response.
async fn handle_post_order(
    State(state): State<Arc<MockClobState>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    {
        let mut orders = state.orders_received.lock().unwrap();
        orders.push(body);
    }
    Json(json!({
        "success": true,
        "orderID": "0x0000000000000000000000000000000000000000000000000000000000000001",
        "status": "live",
        "error_msg": "",
        "makingAmount": "0",
        "takingAmount": "0",
        "transactionHashes": [],
        "tradeIds": []
    }))
}

/// `GET /data/orders` — return empty paginated response.
async fn handle_get_orders() -> Json<Value> {
    Json(json!({
        "data": [],
        "next_cursor": "LTE="
    }))
}

/// `DELETE /order` — cancel a single order. Returns the order ID from the body.
async fn handle_cancel_order(Json(body): Json<Value>) -> Json<Value> {
    let order_id = body
        .get("orderID")
        .or_else(|| body.get("orderId"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    Json(json!({
        "canceled": [order_id],
        "notCanceled": {}
    }))
}

/// `DELETE /cancel-all` — cancel all orders.
async fn handle_cancel_all() -> Json<Value> {
    Json(json!({
        "canceled": [],
        "notCanceled": {}
    }))
}

/// `DELETE /cancel-market-orders` — cancel orders for a specific market.
async fn handle_cancel_market_orders() -> Json<Value> {
    Json(json!({
        "canceled": [],
        "notCanceled": {}
    }))
}

/// `GET /balance-allowance` — return mock balance of 10 USDC (6 decimals).
async fn handle_balance_allowance() -> Json<Value> {
    Json(json!({
        "balance": "10000000",
        "allowances": {
            "0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E": "unlimited"
        }
    }))
}

/// `GET /data/trades` — return empty paginated response.
async fn handle_get_trades() -> Json<Value> {
    Json(json!({
        "data": [],
        "next_cursor": "LTE="
    }))
}
