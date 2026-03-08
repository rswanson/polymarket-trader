# Integration Testing (Forked Chain + Mock CLOB) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add integration tests that verify CLI order signing and account flows against a forked Polygon chain with a mock CLOB server.

**Architecture:** Anvil forks Polygon mainnet to get real Polymarket contract state. An in-process axum mock CLOB server accepts signed orders, validates EIP-712 signatures, and settles against the fork's CTF Exchange contract. Tests run the CLI binary pointing at the mock server, using a local private key instead of AWS KMS.

**Tech Stack:** Rust, alloy (Anvil bindings + contract interaction), axum (mock HTTP server), Polymarket CTF Exchange / USDC / Conditional Tokens contracts on forked Polygon, GitHub Actions + Foundry toolchain.

---

## Contract Addresses (Polygon Mainnet)

These are the real addresses that exist on the forked chain:

- **CTF Exchange**: `0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E`
- **Neg Risk CTF Exchange**: `0xC5d563A36AE78145C45a50134d48A1215220f80a`
- **Conditional Tokens (CTF)**: `0x4D97DCd97eC945f40cF65F87097ACe5EA0476045`
- **USDC.e**: `0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174`

---

### Task 1: Add `--private-key` CLI Option

**Files:**
- Modify: `src/cli.rs:5-25` (add private_key arg)
- Modify: `src/signer.rs` (add local signer constructor)
- Modify: `src/main.rs:112-192` (signer selection logic)

**Step 1: Add private_key field to CLI struct**

In `src/cli.rs`, add to the `Cli` struct:

```rust
/// Local private key for wallet signing (alternative to KMS)
#[arg(long, env = "POLYMARKET_PRIVATE_KEY", global = true)]
pub private_key: Option<String>,
```

**Step 2: Add local signer constructor in `src/signer.rs`**

Add a new function alongside `create_kms_signer`:

```rust
use alloy::signers::local::PrivateKeySigner;

pub fn create_local_signer(private_key: &str) -> anyhow::Result<PrivateKeySigner> {
    let key = private_key.strip_prefix("0x").unwrap_or(private_key);
    let signer: PrivateKeySigner = key
        .parse()
        .map_err(|e| anyhow::anyhow!("Failed to parse private key: {e}"))?;
    Ok(signer)
}
```

**Step 3: Update main.rs signer selection**

Replace the signer creation in the `Command::Orders` and `Command::Account` arms. The existing code gets `kms_key_id` and calls `create_kms_signer`. Change both arms to use a helper that selects the signer:

In `src/main.rs`, before the `run` function, add:

```rust
use alloy::signers::local::PrivateKeySigner;

enum WalletSigner {
    Kms(alloy::signers::aws::AwsSigner),
    Local(PrivateKeySigner),
}
```

However, since `create_authenticated_client` and `place_limit`/`place_market` are generic over `S: Signer`, and we need to pass the signer to multiple calls, the simplest approach is to use a dynamic dispatch wrapper or duplicate the match arms. The cleanest approach: use `Box<dyn Signer + Send + Sync>` — but check first if `alloy::signers::Signer` is object-safe.

If not object-safe (likely due to associated types), use an enum:

```rust
// In src/signer.rs
use alloy::signers::{Signer, Signature};
use alloy::primitives::B256;

pub enum AnySigner {
    Kms(AwsSigner),
    Local(PrivateKeySigner),
}

impl Signer for AnySigner {
    // Delegate to inner signer
    async fn sign_hash(&self, hash: &B256) -> alloy::signers::Result<Signature> {
        match self {
            Self::Kms(s) => s.sign_hash(hash).await,
            Self::Local(s) => s.sign_hash(hash).await,
        }
    }

    fn address(&self) -> alloy::primitives::Address {
        match self {
            Self::Kms(s) => s.address(),
            Self::Local(s) => s.address(),
        }
    }

    fn chain_id(&self) -> Option<u64> {
        match self {
            Self::Kms(s) => s.chain_id(),
            Self::Local(s) => s.chain_id(),
        }
    }

    fn set_chain_id(&mut self, chain_id: Option<u64>) {
        match self {
            Self::Kms(s) => s.set_chain_id(chain_id),
            Self::Local(s) => s.set_chain_id(chain_id),
        }
    }
}
```

Then update `main.rs` to resolve the signer once for each auth-requiring arm:

```rust
// Replace the kms_key_id resolution block in both Orders and Account arms with:
let signer = match (&cli.private_key, &cli.kms_key_id) {
    (Some(_), Some(_)) => {
        anyhow::bail!("Cannot specify both --private-key and --kms-key-id");
    }
    (Some(pk), None) => signer::AnySigner::Local(signer::create_local_signer(pk)?),
    (None, Some(key_id)) => {
        signer::AnySigner::Kms(signer::create_kms_signer(key_id).await?)
    }
    (None, None) => {
        anyhow::bail!(
            "Wallet key is required for this command. \
             Set --private-key / POLYMARKET_PRIVATE_KEY or \
             --kms-key-id / POLYMARKET_KMS_KEY_ID."
        );
    }
};
let client = client::create_authenticated_client(&cli.clob_host, &signer).await?;
```

**Step 4: Update alloy features in Cargo.toml**

```toml
alloy = { version = "1.6", features = ["signer-aws", "signer-local"] }
```

**Step 5: Verify it compiles**

Run: `cargo build`
Expected: Compiles without errors.

**Step 6: Run existing tests**

Run: `cargo test`
Expected: All existing tests pass. The `orders_without_kms_key_fails` and `account_without_kms_key_fails` tests will need updating since the error message changes.

Update `tests/cli.rs` — change both tests to check for the new error message:

```rust
#[test]
fn orders_without_wallet_key_fails() {
    cmd()
        .args(["orders", "list"])
        .env_remove("POLYMARKET_KMS_KEY_ID")
        .env_remove("POLYMARKET_PRIVATE_KEY")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Wallet key is required"));
}

#[test]
fn account_without_wallet_key_fails() {
    cmd()
        .args(["account", "balance"])
        .env_remove("POLYMARKET_KMS_KEY_ID")
        .env_remove("POLYMARKET_PRIVATE_KEY")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Wallet key is required"));
}
```

**Step 7: Add mutual exclusion test**

```rust
#[test]
fn both_private_key_and_kms_fails() {
    cmd()
        .args(["orders", "list"])
        .env("POLYMARKET_KMS_KEY_ID", "some-key-id")
        .env("POLYMARKET_PRIVATE_KEY", "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Cannot specify both"));
}
```

**Step 8: Commit**

```bash
git add src/cli.rs src/signer.rs src/main.rs Cargo.toml tests/cli.rs
git commit -m "feat: add --private-key as alternative to --kms-key-id for local wallet signing"
```

---

### Task 2: Add Integration Test Dependencies and Feature Gate

**Files:**
- Modify: `Cargo.toml` (add dev-dependencies and feature)

**Step 1: Add the integration-tests feature and dev-dependencies**

In `Cargo.toml`, add:

```toml
[features]
default = []
integration-tests = []

[dev-dependencies]
assert_cmd = "2"
predicates = "3"
axum = "0.8"
tokio = { version = "1", features = ["full"] }
alloy = { version = "1.6", features = ["provider-anvil-node", "sol-types", "contract", "signer-local", "provider-http", "rpc-types"] }
serde_json = "1"
```

Note: `alloy`'s `provider-anvil-node` feature includes `AnvilInstance` for spawning Anvil programmatically.

**Step 2: Verify it compiles with the feature**

Run: `cargo test --features integration-tests`
Expected: Compiles (no new tests yet, existing tests still pass).

**Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "chore: add integration-tests feature and dev-dependencies (axum, alloy test utils)"
```

---

### Task 3: Anvil Fork Setup Module

**Files:**
- Create: `tests/integration/mod.rs`
- Create: `tests/integration/fork_setup.rs`

**Step 1: Create the integration test module**

`tests/integration/mod.rs`:

```rust
#![cfg(feature = "integration-tests")]

pub mod fork_setup;
```

**Step 2: Write fork_setup.rs**

This module provides helpers to:
- Spawn Anvil forking Polygon mainnet
- Fund a test account with USDC using `anvil_setStorageAt` (manipulate USDC contract storage to give the test wallet a balance)
- Approve CTF Exchange to spend USDC

```rust
use alloy::node_bindings::Anvil;
use alloy::primitives::{Address, U256, address, keccak256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::sol;
use alloy::signers::local::PrivateKeySigner;
use std::time::Duration;

// Polymarket contract addresses on Polygon mainnet
pub const CTF_EXCHANGE: Address = address!("4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E");
pub const NEG_RISK_CTF_EXCHANGE: Address = address!("C5d563A36AE78145C45a50134d48A1215220f80a");
pub const CONDITIONAL_TOKENS: Address = address!("4D97DCd97eC945f40cF65F87097ACe5EA0476045");
pub const USDC: Address = address!("2791Bca1f2de4661ED88A30C99A7a9449Aa84174");

// Minimal ERC20 interface for USDC interactions
sol! {
    #[sol(rpc)]
    interface IERC20 {
        function balanceOf(address account) external view returns (uint256);
        function approve(address spender, uint256 amount) external returns (bool);
        function allowance(address owner, address spender) external view returns (uint256);
    }
}

pub struct ForkEnv {
    pub anvil: Anvil,
    pub rpc_url: String,
    pub signer: PrivateKeySigner,
}

/// Spawn an Anvil instance forking Polygon mainnet.
/// Requires POLYGON_RPC_URL env var (e.g., an Alchemy or Infura endpoint).
pub async fn spawn_fork() -> ForkEnv {
    let rpc_url = std::env::var("POLYGON_RPC_URL")
        .expect("POLYGON_RPC_URL must be set for integration tests");

    let anvil = Anvil::new()
        .fork(&rpc_url)
        .timeout(Duration::from_secs(60))
        .spawn();

    let signer: PrivateKeySigner = anvil.keys()[0].clone().into();

    ForkEnv {
        rpc_url: anvil.endpoint(),
        anvil,
        signer,
    }
}

/// Fund the test wallet with USDC by manipulating contract storage.
/// USDC.e uses a mapping at storage slot 0 for balances (standard ERC20 proxy pattern).
/// The actual slot may differ — we try the common slots and verify.
pub async fn fund_usdc(fork: &ForkEnv, amount: U256) {
    let provider = ProviderBuilder::new().on_http(fork.rpc_url.parse().unwrap());
    let wallet_address = fork.signer.address();

    // USDC.e (bridged) on Polygon uses slot 0 for balances mapping
    // Storage slot for balanceOf[address] = keccak256(abi.encode(address, slot))
    // Try common slots: 0, 2, 9 (varies by proxy implementation)
    for slot in [0u64, 2, 9] {
        let storage_key = keccak256(
            &[&wallet_address.0 .0 as &[u8], &U256::from(slot).to_be_bytes::<32>()].concat(),
        );

        provider
            .anvil_set_storage_at(USDC, storage_key.into(), amount)
            .await
            .unwrap();

        // Verify it worked
        let erc20 = IERC20::new(USDC, &provider);
        let balance = erc20.balanceOf(wallet_address).call().await.unwrap();
        if balance._0 == amount {
            return;
        }
    }

    panic!("Failed to set USDC balance — storage slot not found");
}

/// Approve the CTF Exchange to spend USDC on behalf of the test wallet.
pub async fn approve_exchange(fork: &ForkEnv) {
    let provider = ProviderBuilder::new()
        .wallet(alloy::network::EthereumWallet::new(fork.signer.clone()))
        .on_http(fork.rpc_url.parse().unwrap());

    let erc20 = IERC20::new(USDC, &provider);
    let max_approval = U256::MAX;

    erc20.approve(CTF_EXCHANGE, max_approval).send().await.unwrap()
        .get_receipt().await.unwrap();
    erc20.approve(NEG_RISK_CTF_EXCHANGE, max_approval).send().await.unwrap()
        .get_receipt().await.unwrap();
}
```

**Step 3: Verify it compiles**

Run: `cargo test --features integration-tests --no-run`
Expected: Compiles without errors.

**Step 4: Commit**

```bash
git add tests/integration/
git commit -m "feat: add Anvil fork setup for integration tests (USDC funding, exchange approvals)"
```

---

### Task 4: Mock CLOB Server — Auth Endpoints

**Files:**
- Create: `tests/integration/mock_clob.rs`
- Modify: `tests/integration/mod.rs`

**Step 1: Create the mock CLOB server scaffold with auth endpoints**

The mock needs to handle the SDK's authentication flow. The SDK calls:
1. `POST /auth/l1` — submits an EIP-712 signature to prove wallet ownership, gets a nonce
2. `POST /auth/l2` — submits HMAC credentials derived from the L1 auth

For testing, the mock can accept any well-formed auth request and return valid-looking credentials.

`tests/integration/mock_clob.rs`:

```rust
use axum::{Router, Json, extract::State};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct MockClobState {
    pub rpc_url: String,
    pub orders_received: Mutex<Vec<serde_json::Value>>,
}

#[derive(Deserialize)]
struct AuthRequest {
    // The SDK sends various fields; we accept anything
    #[serde(flatten)]
    _fields: serde_json::Value,
}

#[derive(Serialize)]
struct AuthL1Response {
    nonce: String,
}

#[derive(Serialize)]
struct AuthL2Response {
    #[serde(rename = "apiKey")]
    api_key: String,
    secret: String,
    passphrase: String,
}

async fn auth_l1(Json(_body): Json<AuthRequest>) -> Json<AuthL1Response> {
    Json(AuthL1Response {
        nonce: "test-nonce-12345".to_string(),
    })
}

async fn auth_l2(Json(_body): Json<AuthRequest>) -> Json<AuthL2Response> {
    Json(AuthL2Response {
        api_key: "test-api-key".to_string(),
        secret: "dGVzdC1zZWNyZXQ=".to_string(), // base64 "test-secret"
        passphrase: "test-passphrase".to_string(),
    })
}

pub async fn start_mock_clob(rpc_url: String) -> (SocketAddr, Arc<MockClobState>) {
    let state = Arc::new(MockClobState {
        rpc_url,
        orders_received: Mutex::new(Vec::new()),
    });

    let app = Router::new()
        .route("/auth/l1", post(auth_l1))
        .route("/auth/l2", post(auth_l2))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (addr, state)
}
```

Note: The exact auth endpoint paths and request/response shapes need to be verified against the `polymarket-client-sdk` source. The SDK may use `/auth/derive-api-key` or similar. This will need adjustment during implementation — inspect the SDK's HTTP calls by running with `RUST_LOG=debug` or reading the SDK source.

**Step 2: Update mod.rs**

```rust
#![cfg(feature = "integration-tests")]

pub mod fork_setup;
pub mod mock_clob;
```

**Step 3: Verify it compiles**

Run: `cargo test --features integration-tests --no-run`
Expected: Compiles.

**Step 4: Commit**

```bash
git add tests/integration/mock_clob.rs tests/integration/mod.rs
git commit -m "feat: add mock CLOB server with auth endpoints for integration tests"
```

---

### Task 5: Mock CLOB Server — Order and Balance Endpoints

**Files:**
- Modify: `tests/integration/mock_clob.rs`

**Step 1: Add order submission endpoint**

The mock CLOB's `POST /order` endpoint should:
1. Accept the signed order JSON
2. Store it for later assertions
3. Optionally validate the EIP-712 signature against the fork
4. Return a success response matching the SDK's expected format

Add to `mock_clob.rs`:

```rust
use alloy::primitives::Address;

#[derive(Serialize)]
struct PostOrderResponse {
    success: bool,
    #[serde(rename = "orderID")]
    order_id: String,
    #[serde(rename = "errorMsg")]
    error_msg: Option<String>,
}

#[derive(Serialize)]
struct BalanceAllowanceResponse {
    balance: String,
    allowances: std::collections::HashMap<String, String>,
}

async fn post_order(
    State(state): State<Arc<MockClobState>>,
    Json(body): Json<serde_json::Value>,
) -> Json<PostOrderResponse> {
    // Store the order for test assertions
    state.orders_received.lock().await.push(body);

    Json(PostOrderResponse {
        success: true,
        order_id: uuid::Uuid::new_v4().to_string(),
        error_msg: None,
    })
}

async fn balance_allowance(
    State(_state): State<Arc<MockClobState>>,
) -> Json<BalanceAllowanceResponse> {
    // Return a mock balance — in a more sophisticated version,
    // this would read from the forked chain
    let mut allowances = std::collections::HashMap::new();
    allowances.insert(
        "0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E".to_string(),
        "1000000000".to_string(), // max allowance
    );

    Json(BalanceAllowanceResponse {
        balance: "10000000".to_string(), // 10 USDC (6 decimals)
        allowances,
    })
}

// Cancel endpoints
async fn cancel_order_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "canceled": ["mock-order-id"],
        "not_canceled": {}
    }))
}

async fn cancel_all_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "canceled": [],
        "not_canceled": {}
    }))
}
```

**Step 2: Wire up routes in `start_mock_clob`**

Update the Router:

```rust
let app = Router::new()
    .route("/auth/l1", post(auth_l1))
    .route("/auth/l2", post(auth_l2))
    .route("/order", post(post_order))
    .route("/balance-allowance", get(balance_allowance))
    .route("/order/{order_id}", axum::routing::delete(cancel_order_handler))
    .route("/cancel-all", post(cancel_all_handler))
    .with_state(state.clone());
```

Note: The exact route paths need to match what `polymarket-client-sdk` sends. Verify by reading the SDK source or running with `RUST_LOG=trace`. Common CLOB paths: `/order`, `/cancel`, `/cancel-all`, `/balance-allowance`. Adjust during implementation.

**Step 3: Add uuid to dev-dependencies**

In `Cargo.toml` dev-dependencies, `uuid` is already a regular dependency, so it's available in tests.

**Step 4: Verify it compiles**

Run: `cargo test --features integration-tests --no-run`

**Step 5: Commit**

```bash
git add tests/integration/mock_clob.rs
git commit -m "feat: add order and balance endpoints to mock CLOB server"
```

---

### Task 6: Write Auth Flow Integration Test

**Files:**
- Create: `tests/integration/test_auth.rs`
- Modify: `tests/integration/mod.rs`

**Step 1: Write the test**

This test verifies the CLI can authenticate against the mock CLOB using a local private key.

`tests/integration/test_auth.rs`:

```rust
use assert_cmd::Command;

use super::fork_setup;
use super::mock_clob;

fn cmd() -> Command {
    Command::cargo_bin("polymarket-trader").unwrap()
}

/// Test that the CLI can authenticate and list orders (empty) against the mock CLOB.
#[tokio::test]
async fn auth_and_list_orders() {
    let fork = fork_setup::spawn_fork().await;
    let (addr, _state) = mock_clob::start_mock_clob(fork.rpc_url.clone()).await;

    let private_key = hex::encode(fork.signer.credential().to_bytes());

    // The orders list command requires auth — if auth works, we get an empty list
    // rather than an auth error
    let output = cmd()
        .env("POLYMARKET_PRIVATE_KEY", &private_key)
        .env("POLYMARKET_CLOB_HOST", format!("http://{addr}"))
        .args(["--json", "orders", "list"])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    // Should succeed (auth worked, got empty order list from mock)
    assert!(
        output.status.success(),
        "Expected success but got:\nstdout: {stdout}\nstderr: {stderr}"
    );
}
```

Note: This test will likely need iteration. The mock auth endpoints must match exactly what the SDK expects. Run with `RUST_LOG=debug` to see what the SDK sends. The mock may need additional endpoints like `/time` (server time) or different auth paths.

**Step 2: Add hex to dev-dependencies**

```toml
hex = "0.4"
```

**Step 3: Update mod.rs**

```rust
#![cfg(feature = "integration-tests")]

pub mod fork_setup;
pub mod mock_clob;
pub mod test_auth;
```

**Step 4: Run the test (requires POLYGON_RPC_URL)**

Run: `POLYGON_RPC_URL=<your-rpc-url> cargo test --features integration-tests test_auth -- --nocapture`
Expected: The test either passes or gives clear errors about which mock endpoints need adjustment.

**Step 5: Iterate on mock endpoints**

This is the discovery step. Run with `RUST_LOG=trace` to see exactly what HTTP requests the SDK makes during authentication. Adjust the mock's routes and response shapes to match. This may take several iterations.

**Step 6: Commit**

```bash
git add tests/integration/test_auth.rs tests/integration/mod.rs Cargo.toml
git commit -m "feat: add auth flow integration test against mock CLOB"
```

---

### Task 7: Write Order Placement Integration Test

**Files:**
- Create: `tests/integration/test_orders.rs`
- Modify: `tests/integration/mod.rs`

**Step 1: Write the test**

`tests/integration/test_orders.rs`:

```rust
use assert_cmd::Command;

use super::fork_setup;
use super::mock_clob;

fn cmd() -> Command {
    Command::cargo_bin("polymarket-trader").unwrap()
}

/// Test placing a limit order through the full CLI flow.
/// Uses a real token ID from a known Polymarket market.
#[tokio::test]
async fn place_limit_order() {
    let fork = fork_setup::spawn_fork().await;
    fork_setup::fund_usdc(&fork, alloy::primitives::U256::from(10_000_000u64)).await; // 10 USDC
    fork_setup::approve_exchange(&fork).await;

    let (addr, state) = mock_clob::start_mock_clob(fork.rpc_url.clone()).await;
    let private_key = hex::encode(fork.signer.credential().to_bytes());

    // Use a known token ID from a real market on the fork
    // This is a placeholder — pick a real active market's token ID
    let token_id = "52114319501245915516055106046884209969926127482827954674443846427813813222426";

    let output = cmd()
        .env("POLYMARKET_PRIVATE_KEY", &private_key)
        .env("POLYMARKET_CLOB_HOST", format!("http://{addr}"))
        .args([
            "--json", "orders", "limit",
            token_id, // market (using token ID directly)
            "buy", "0.50", "10",
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    assert!(
        output.status.success(),
        "Expected success but got:\nstdout: {stdout}\nstderr: {stderr}"
    );

    // Verify the mock received the signed order
    let orders = state.orders_received.lock().await;
    assert_eq!(orders.len(), 1, "Expected exactly one order to be posted");

    // Parse and verify the order has expected fields
    let order = &orders[0];
    assert!(order.get("order").is_some() || order.get("signed_order").is_some(),
        "Order payload should contain order data: {order}");
}

/// Test that placing an order without auth fails gracefully
#[tokio::test]
async fn place_order_no_auth_fails() {
    let output = cmd()
        .env_remove("POLYMARKET_KMS_KEY_ID")
        .env_remove("POLYMARKET_PRIVATE_KEY")
        .args(["orders", "limit", "some-token", "buy", "0.50", "10"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("Wallet key is required"));
}
```

Note: The `orders limit` command takes market as first positional arg. Check `cli.rs:140-152` — it's `market`, `side`, `price`, `size`. But the `market` arg goes through `resolve_market` which tries Gamma API first. For integration tests, we need to either:
- Pass a raw token ID and ensure it bypasses Gamma resolution, OR
- Also mock the Gamma API endpoints

This is an important detail to resolve during implementation. The simplest path: pass a token ID directly if `resolve_market` falls back to treating unknown strings as token IDs.

**Step 2: Update mod.rs**

```rust
pub mod test_orders;
```

**Step 3: Run the test**

Run: `POLYGON_RPC_URL=<your-rpc-url> cargo test --features integration-tests test_orders -- --nocapture`

**Step 4: Commit**

```bash
git add tests/integration/test_orders.rs tests/integration/mod.rs
git commit -m "feat: add order placement integration tests against mock CLOB + fork"
```

---

### Task 8: Write Account Balance Integration Test

**Files:**
- Create: `tests/integration/test_account.rs`
- Modify: `tests/integration/mod.rs`

**Step 1: Write the test**

`tests/integration/test_account.rs`:

```rust
use assert_cmd::Command;

use super::fork_setup;
use super::mock_clob;

fn cmd() -> Command {
    Command::cargo_bin("polymarket-trader").unwrap()
}

/// Test querying account balance through the CLI.
#[tokio::test]
async fn query_balance() {
    let fork = fork_setup::spawn_fork().await;
    let (addr, _state) = mock_clob::start_mock_clob(fork.rpc_url.clone()).await;
    let private_key = hex::encode(fork.signer.credential().to_bytes());

    let output = cmd()
        .env("POLYMARKET_PRIVATE_KEY", &private_key)
        .env("POLYMARKET_CLOB_HOST", format!("http://{addr}"))
        .args(["--json", "account", "balance"])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    assert!(
        output.status.success(),
        "Expected success but got:\nstdout: {stdout}\nstderr: {stderr}"
    );

    // Verify we got a balance response
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect("Output should be valid JSON");
    assert!(parsed.get("balance").is_some(), "Response should contain balance");
}
```

**Step 2: Update mod.rs**

```rust
pub mod test_account;
```

**Step 3: Run the test**

Run: `POLYGON_RPC_URL=<your-rpc-url> cargo test --features integration-tests test_account -- --nocapture`

**Step 4: Commit**

```bash
git add tests/integration/test_account.rs tests/integration/mod.rs
git commit -m "feat: add account balance integration test against mock CLOB"
```

---

### Task 9: Add CI Job

**Files:**
- Modify: `.github/workflows/ci.yml`

**Step 1: Add integration-test job**

Add after the existing `build` job:

```yaml
  integration-test:
    name: Integration Tests
    needs: [check, clippy, fmt]
    runs-on: ubuntu-latest
    # Don't block PRs if RPC provider is down
    continue-on-error: true
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Install Foundry
        uses: foundry-rs/foundry-toolchain@v1
      - name: Run integration tests
        env:
          POLYGON_RPC_URL: ${{ secrets.POLYGON_RPC_URL }}
          RUST_LOG: warn
        run: cargo test --features integration-tests -- --nocapture
        timeout-minutes: 10
```

**Step 2: Verify the workflow syntax**

Run: `cd /Users/swanpro/git/polymarket-trader && cat .github/workflows/ci.yml` to verify YAML is valid.

**Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add integration test job with Anvil fork and mock CLOB"
```

---

### Task 10: Documentation and Cleanup

**Files:**
- Modify: `CLAUDE.md` (update build commands and test info)

**Step 1: Update CLAUDE.md**

Add to the Build & Development Commands section:

```bash
cargo test --features integration-tests  # Integration tests (requires POLYGON_RPC_URL)
```

Update the "No test suite yet" note to reflect reality:

```
Tests: `cargo test` runs CLI integration tests. `cargo test --features integration-tests` runs
chain fork tests (requires `POLYGON_RPC_URL` env var pointing to a Polygon RPC endpoint).
```

**Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md with integration test instructions"
```

---

## Implementation Notes

### SDK Discovery Required

Several mock endpoints need to exactly match what `polymarket-client-sdk` sends. The plan provides best-guess endpoints based on common CLOB API patterns, but the implementer **must**:

1. Run the CLI with `RUST_LOG=trace` against the mock to see actual HTTP requests
2. Read the `polymarket-client-sdk` source (check `~/.cargo/registry/src/` for the downloaded crate)
3. Adjust mock routes and response schemas accordingly

Key unknowns:
- Exact auth endpoint paths (`/auth/l1` vs `/auth/derive-api-key` vs something else)
- Auth request/response JSON shapes
- Order submission request format (the SDK may wrap the signed order)
- Balance/allowance endpoint path and query parameters
- Whether the SDK calls `/time` or `/server-time` for server time sync

### Gamma API Resolution

The `orders limit` command resolves market slugs via the Gamma API before creating orders. For integration tests using raw token IDs, check if `resolve_market` handles the case where the input is already a token ID. If not, either:
- Add a `--token-id` flag to bypass resolution, or
- Also mock the relevant Gamma API endpoint

### Storage Slot Discovery

The `fund_usdc` function tries common ERC20 storage slots. If none work, use `cast storage` against the forked chain to find USDC.e's actual balance mapping slot:

```bash
cast storage 0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174 --rpc-url <anvil-url>
```

---

Plan complete and saved to `docs/plans/2026-03-08-integration-testing-plan.md`. Two execution options:

**1. Subagent-Driven (this session)** — I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Parallel Session (separate)** — Open new session with executing-plans, batch execution with checkpoints

Which approach?
