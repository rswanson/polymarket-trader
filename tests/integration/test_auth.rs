//! Integration test: authentication flow against the mock CLOB server.

use crate::integration::mock_clob::start_mock_clob;
use alloy::signers::local::PrivateKeySigner;
use std::time::Duration;

/// Helper to run the CLI binary as a subprocess without blocking the tokio runtime.
/// Uses `tokio::process::Command` so the mock server can continue serving requests.
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

/// Quick sanity check that the mock CLOB server responds to requests.
#[tokio::test]
async fn mock_clob_responds() {
    let (addr, _state) = start_mock_clob("http://localhost:8545".to_string()).await;
    let url = format!("http://127.0.0.1:{}/time", addr.port());

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .expect("request to mock /time failed");

    let status = resp.status();
    let body = resp.text().await.expect("failed to read body");
    eprintln!("mock /time status={status} body={body:?}");

    assert!(status.is_success());
    assert!(
        body.parse::<i64>().is_ok(),
        "Expected numeric body, got: {body:?}"
    );
}

/// Test that the CLI can authenticate against the mock CLOB using a local
/// private key and successfully list orders (empty result).
#[tokio::test]
async fn auth_and_list_orders() {
    // 1. Start mock CLOB (no Anvil fork needed for auth-only tests)
    let (addr, _state) = start_mock_clob("http://localhost:8545".to_string()).await;
    let clob_host = format!("http://127.0.0.1:{}", addr.port());

    // 2. Generate a random private key
    let signer = PrivateKeySigner::random();
    let key_hex = hex::encode(signer.credential().to_bytes());

    // 3. Run: polymarket-trader --json --private-key <key> --clob-host <url> orders list
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
                "list",
            ],
            &key_hex,
            &clob_host,
        ),
    )
    .await
    .expect("CLI timed out after 15s");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // 4. Assert success (auth worked, got empty order list)
    assert!(
        output.status.success(),
        "CLI exited with failure.\nstdout: {stdout}\nstderr: {stderr}"
    );

    // 5. Check stdout contains "[]" (empty JSON array)
    assert!(
        stdout.contains("[]"),
        "Expected empty JSON array in output.\nstdout: {stdout}"
    );
}
