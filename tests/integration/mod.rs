#![cfg(feature = "integration-tests")]

#[allow(dead_code)]
pub mod fork_setup;
pub mod mock_clob;
pub mod test_account;
pub mod test_auth;
pub mod test_orders;

/// Run the CLI binary as a subprocess without blocking the tokio runtime.
/// Uses `tokio::process::Command` so the mock server can continue serving requests.
pub async fn run_cli(args: &[&str], key_hex: &str, clob_host: &str) -> std::process::Output {
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
