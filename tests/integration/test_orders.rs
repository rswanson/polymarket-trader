//! Integration test: order placement against the mock CLOB server.

use crate::integration::mock_clob::start_mock_clob;
use alloy::signers::local::PrivateKeySigner;
use std::time::Duration;

/// Helper to run the CLI binary as a subprocess without blocking the tokio runtime.
async fn run_cli(args: &[&str], key_hex: &str, clob_host: &str) -> std::process::Output {
    let bin_path = assert_cmd::cargo_bin!("polymarket-trader");
    tokio::process::Command::new(bin_path)
        .args(args)
        .env("POLYMARKET_PRIVATE_KEY", key_hex)
        .env_remove("POLYMARKET_KMS_KEY_ID")
        .env_remove("RUST_LOG")
        .env("POLYMARKET_CLOB_HOST", clob_host)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("failed to spawn CLI")
        .wait_with_output()
        .await
        .expect("failed to wait for CLI")
}

/// Test placing a limit order against the mock CLOB.
///
/// The `orders limit` command calls `resolve_market()` which, for a raw
/// token ID (all digits), does a best-effort Gamma API reverse lookup that
/// gracefully falls through on failure. The SDK's `limit_order().build()`
/// calls `/fee-rate` and `/tick-size`, and `sign()` calls `/neg-risk` --
/// all handled by the mock.
#[tokio::test]
async fn place_limit_order() {
    // 1. Start mock CLOB
    let (addr, state) = start_mock_clob("http://localhost:8545".to_string()).await;
    let clob_host = format!("http://127.0.0.1:{}", addr.port());

    // 2. Generate random private key
    let signer = PrivateKeySigner::random();
    let key_hex = hex::encode(signer.credential().to_bytes());

    // Use a realistic token ID (all digits, so resolve_market will treat it as raw)
    let token_id =
        "52114319501245915516055106046884209969926127482827954674443846427813813222426";

    // 3. Run: polymarket-trader --json --private-key <key> --clob-host <url> orders limit <token_id> buy 0.50 10
    let output = tokio::time::timeout(
        Duration::from_secs(15),
        run_cli(
            &[
                "--json",
                "--private-key",
                &key_hex,
                "--clob-host",
                &clob_host,
                "orders",
                "limit",
                token_id,
                "buy",
                "0.50",
                "10",
            ],
            &key_hex,
            &clob_host,
        ),
    )
    .await
    .expect("CLI timed out after 15s");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // 4. Assert success, check mock received 1 order
    assert!(
        output.status.success(),
        "CLI exited with failure.\nstdout: {stdout}\nstderr: {stderr}"
    );

    assert!(
        stdout.contains("success"),
        "Expected 'success' in output.\nstdout: {stdout}"
    );

    // Verify mock received the order
    let orders = state.orders_received.lock().unwrap();
    assert_eq!(
        orders.len(),
        1,
        "Expected 1 order received by mock, got {}",
        orders.len()
    );
}

/// Test that running an authenticated command without any key fails with a
/// helpful error message.
#[tokio::test]
async fn place_order_no_auth_fails() {
    let bin_path = assert_cmd::cargo_bin!("polymarket-trader");
    let output = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::process::Command::new(bin_path)
            .args(["--json", "orders", "list"])
            .env_remove("POLYMARKET_KMS_KEY_ID")
            .env_remove("POLYMARKET_PRIVATE_KEY")
            .env_remove("RUST_LOG")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("failed to spawn CLI")
            .wait_with_output()
            .await
            .expect("failed to wait for CLI")
    })
    .await
    .expect("CLI timed out after 10s");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        !output.status.success(),
        "Expected CLI to fail without auth credentials"
    );
    assert!(
        combined.contains("Wallet key is required"),
        "Expected 'Wallet key is required' error.\nstdout: {stdout}\nstderr: {stderr}"
    );
}
