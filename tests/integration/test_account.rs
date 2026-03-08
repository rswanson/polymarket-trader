//! Integration test: account balance and trades queries against the mock CLOB.

use crate::integration::mock_clob::start_mock_clob;
use crate::integration::run_cli;
use alloy::signers::local::PrivateKeySigner;
use std::time::Duration;

/// Test querying account balance against the mock CLOB.
#[tokio::test]
async fn query_balance() {
    // 1. Start mock CLOB
    let (addr, _state) = start_mock_clob("http://localhost:8545".to_string()).await;
    let clob_host = format!("http://127.0.0.1:{}", addr.port());

    // 2. Generate random private key
    let signer = PrivateKeySigner::random();
    let key_hex = hex::encode(signer.credential().to_bytes());

    // 3. Run: polymarket-trader --json --private-key <key> --clob-host <url> account balance
    let output = tokio::time::timeout(
        Duration::from_secs(15),
        run_cli(
            &[
                "--json",
                "--private-key",
                &key_hex,
                "--clob-host",
                &clob_host,
                "account",
                "balance",
            ],
            &key_hex,
            &clob_host,
        ),
    )
    .await
    .expect("CLI timed out after 15s");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // 4. Assert success, parse JSON output, verify "balance" field exists
    assert!(
        output.status.success(),
        "CLI exited with failure.\nstdout: {stdout}\nstderr: {stderr}"
    );

    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Failed to parse JSON output");
    assert!(
        parsed.get("balance").is_some(),
        "Expected 'balance' field in JSON output.\nparsed: {parsed:#}"
    );
}

/// Test listing trades (should return empty array from mock).
#[tokio::test]
async fn list_trades_empty() {
    // 1. Start mock CLOB
    let (addr, _state) = start_mock_clob("http://localhost:8545".to_string()).await;
    let clob_host = format!("http://127.0.0.1:{}", addr.port());

    // 2. Generate random private key
    let signer = PrivateKeySigner::random();
    let key_hex = hex::encode(signer.credential().to_bytes());

    // 3. Run: polymarket-trader --json --private-key <key> --clob-host <url> account trades
    let output = tokio::time::timeout(
        Duration::from_secs(15),
        run_cli(
            &[
                "--json",
                "--private-key",
                &key_hex,
                "--clob-host",
                &clob_host,
                "account",
                "trades",
            ],
            &key_hex,
            &clob_host,
        ),
    )
    .await
    .expect("CLI timed out after 15s");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // 4. Assert success, verify returns empty array
    assert!(
        output.status.success(),
        "CLI exited with failure.\nstdout: {stdout}\nstderr: {stderr}"
    );

    assert!(
        stdout.contains("[]"),
        "Expected empty JSON array in output.\nstdout: {stdout}"
    );
}
