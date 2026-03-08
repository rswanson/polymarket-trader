use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::path::PathBuf;

fn cmd() -> Command {
    cargo_bin_cmd!("polymarket-trader")
}

/// Use a unique temp DB per test to avoid concurrency conflicts.
fn with_temp_db(test_name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("polymarket-test").join(test_name);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("dry-run.db")
}

// ── Help and basic CLI ──

#[test]
fn help_flag_succeeds() {
    cmd().arg("--help").assert().success().stdout(
        predicate::str::contains("Polymarket trading CLI")
            .and(predicate::str::contains("markets"))
            .and(predicate::str::contains("dry-run")),
    );
}

#[test]
fn no_args_shows_help() {
    cmd()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

#[test]
fn invalid_subcommand_fails() {
    cmd()
        .arg("nonexistent")
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

// ── Dry-run commands ──

#[test]
fn dry_run_reset_json() {
    let db_path = with_temp_db("reset_json");
    // Set HOME to redirect DryRunDb to our temp location
    let home = db_path.parent().unwrap().parent().unwrap();
    std::fs::create_dir_all(home.join(".polymarket")).unwrap();

    cmd()
        .env("HOME", home)
        .args(["--json", "dry-run", "reset", "--balance", "500.00"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("\"balance\": \"500.00\"")
                .and(predicate::str::contains("\"message\"")),
        );
}

#[test]
fn dry_run_reset_table() {
    let db_path = with_temp_db("reset_table");
    let home = db_path.parent().unwrap().parent().unwrap();
    std::fs::create_dir_all(home.join(".polymarket")).unwrap();

    cmd()
        .env("HOME", home)
        .args(["dry-run", "reset", "--balance", "1000.00"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Balance")
                .and(predicate::str::contains("1000.00"))
                .and(predicate::str::contains("Message")),
        );
}

#[test]
fn dry_run_positions_empty_json() {
    let db_path = with_temp_db("positions_empty");
    let home = db_path.parent().unwrap().parent().unwrap();
    std::fs::create_dir_all(home.join(".polymarket")).unwrap();

    // Reset first to get a clean state
    cmd()
        .env("HOME", home)
        .args(["dry-run", "reset"])
        .assert()
        .success();

    cmd()
        .env("HOME", home)
        .args(["--json", "dry-run", "positions"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[]"));
}

#[test]
fn dry_run_trades_empty_json() {
    let db_path = with_temp_db("trades_empty");
    let home = db_path.parent().unwrap().parent().unwrap();
    std::fs::create_dir_all(home.join(".polymarket")).unwrap();

    cmd()
        .env("HOME", home)
        .args(["dry-run", "reset"])
        .assert()
        .success();

    cmd()
        .env("HOME", home)
        .args(["--json", "dry-run", "trades"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[]"));
}

#[test]
fn dry_run_reset_invalid_balance() {
    let db_path = with_temp_db("reset_invalid");
    let home = db_path.parent().unwrap().parent().unwrap();
    std::fs::create_dir_all(home.join(".polymarket")).unwrap();

    cmd()
        .env("HOME", home)
        .args(["dry-run", "reset", "--balance", "not_a_number"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid balance"));
}

#[test]
fn dry_run_cancel_nonexistent_trade() {
    let db_path = with_temp_db("cancel_nonexistent");
    let home = db_path.parent().unwrap().parent().unwrap();
    std::fs::create_dir_all(home.join(".polymarket")).unwrap();

    cmd()
        .env("HOME", home)
        .args(["dry-run", "reset"])
        .assert()
        .success();

    cmd()
        .env("HOME", home)
        .args(["dry-run", "cancel", "fake-id-12345"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// ── Argument validation ──

#[test]
fn orders_without_kms_key_fails() {
    cmd()
        .args(["orders", "list"])
        .env_remove("POLYMARKET_KMS_KEY_ID")
        .assert()
        .failure()
        .stderr(predicate::str::contains("KMS key ID is required"));
}

#[test]
fn account_without_kms_key_fails() {
    cmd()
        .args(["account", "balance"])
        .env_remove("POLYMARKET_KMS_KEY_ID")
        .assert()
        .failure()
        .stderr(predicate::str::contains("KMS key ID is required"));
}

// ── JSON output validation ──

#[test]
fn dry_run_reset_json_is_valid_json() {
    let db_path = with_temp_db("json_valid");
    let home = db_path.parent().unwrap().parent().unwrap();
    std::fs::create_dir_all(home.join(".polymarket")).unwrap();

    let output = cmd()
        .env("HOME", home)
        .args(["--json", "dry-run", "reset", "--balance", "1000.00"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed["balance"], "1000.00");
}

// ── Markets commands (hit real Gamma API) ──

#[test]
fn markets_trending_succeeds() {
    cmd()
        .args(["markets", "trending", "--limit", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Slug"));
}

// ── Subcommand help ──

#[test]
fn dry_run_help() {
    cmd().args(["dry-run", "--help"]).assert().success().stdout(
        predicate::str::contains("limit")
            .and(predicate::str::contains("market"))
            .and(predicate::str::contains("positions"))
            .and(predicate::str::contains("pnl"))
            .and(predicate::str::contains("reset"))
            .and(predicate::str::contains("summary"))
            .and(predicate::str::contains("alerts")),
    );
}

#[test]
fn dry_run_summary_help() {
    cmd()
        .args(["dry-run", "summary", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("trading performance"));
}

#[test]
fn dry_run_alerts_help() {
    cmd()
        .args(["dry-run", "alerts", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("take-profit")
                .and(predicate::str::contains("stop-loss"))
                .and(predicate::str::contains("interval")),
        );
}

#[test]
fn dry_run_help_shows_new_commands() {
    cmd()
        .args(["dry-run", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("summary").and(predicate::str::contains("alerts")));
}

#[test]
fn orders_help() {
    cmd().args(["orders", "--help"]).assert().success().stdout(
        predicate::str::contains("limit")
            .and(predicate::str::contains("market"))
            .and(predicate::str::contains("cancel")),
    );
}
