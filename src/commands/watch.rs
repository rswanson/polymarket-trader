use std::collections::HashMap;
use std::io::{self, Write};

use anyhow::Result;
use chrono::Local;
use polymarket_client_sdk::auth::state::State;
use polymarket_client_sdk::clob::Client;
use polymarket_client_sdk::clob::types::request::MidpointRequest;
use polymarket_client_sdk::types::Decimal;
use serde::Serialize;
use tokio::time::{Duration, interval};

use crate::output::truncate;
use crate::resolve::ResolvedMarket;

#[derive(Serialize)]
struct PriceTick {
    slug: String,
    outcome: String,
    price: String,
    delta: String,
    time: String,
}

fn price_delta(current: Decimal, start: Decimal) -> Decimal {
    current - start
}

fn format_delta(delta: Decimal) -> String {
    if delta >= Decimal::ZERO {
        format!("+{delta}")
    } else {
        format!("{delta}")
    }
}

pub async fn watch<S: State>(
    client: &Client<S>,
    resolved_markets: &[ResolvedMarket],
    interval_secs: u64,
    json: bool,
) -> Result<()> {
    let mut start_prices: HashMap<String, Decimal> = HashMap::new();
    let mut ticker = interval(Duration::from_secs(interval_secs));
    let num_lines = resolved_markets.len();
    let mut first_tick = true;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                // Move cursor up to overwrite previous output (except on first tick)
                if !json && !first_tick {
                    eprint!("\x1B[{}A", num_lines);
                }
                first_tick = false;

                for resolved in resolved_markets {
                    let token_id = resolved.token_id;
                    let request = MidpointRequest::builder().token_id(token_id).build();
                    let mid = match client.midpoint(&request).await {
                        Ok(resp) => resp.mid,
                        Err(e) => {
                            if json {
                                // Skip this tick for this token
                                continue;
                            } else {
                                let label = resolved.slug.as_deref().unwrap_or(&resolved.token_id_str);
                                eprintln!(
                                    "  {} [{}]  Error: {e}",
                                    truncate(label, 50),
                                    resolved.outcome.as_deref().unwrap_or("?")
                                );
                                continue;
                            }
                        }
                    };

                    let start = *start_prices
                        .entry(resolved.token_id_str.clone())
                        .or_insert(mid);
                    let delta = price_delta(mid, start);

                    if json {
                        let tick = PriceTick {
                            slug: resolved.slug.clone().unwrap_or_default(),
                            outcome: resolved.outcome.clone().unwrap_or_default(),
                            price: mid.to_string(),
                            delta: delta.to_string(),
                            time: Local::now().format("%H:%M:%S").to_string(),
                        };
                        println!("{}", serde_json::to_string(&tick)?);
                    } else {
                        let question = resolved
                            .question
                            .as_deref()
                            .or(resolved.slug.as_deref())
                            .unwrap_or(&resolved.token_id_str);
                        let outcome = resolved.outcome.as_deref().unwrap_or("?");
                        let time = Local::now().format("%H:%M:%S");
                        eprintln!(
                            "  {} [{}]  {}  ({})  {}",
                            truncate(question, 50),
                            outcome,
                            mid,
                            format_delta(delta),
                            time
                        );
                    }
                }

                if !json {
                    io::stderr().flush()?;
                }
            }
            _ = tokio::signal::ctrl_c() => {
                break;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn dec(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn price_delta_positive() {
        assert_eq!(price_delta(dec("0.81"), dec("0.80")), dec("0.01"));
    }

    #[test]
    fn price_delta_negative() {
        assert_eq!(price_delta(dec("0.78"), dec("0.80")), dec("-0.02"));
    }

    #[test]
    fn price_delta_zero() {
        assert_eq!(price_delta(dec("0.50"), dec("0.50")), dec("0"));
    }

    #[test]
    fn format_delta_positive() {
        let s = format_delta(dec("0.01"));
        assert!(s.starts_with('+'));
    }

    #[test]
    fn format_delta_negative() {
        let s = format_delta(dec("-0.02"));
        assert!(s.starts_with('-'));
    }

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string() {
        let result = truncate("This is a very long question about something", 20);
        assert!(result.ends_with("..."));
        assert!(result.chars().count() <= 20);
    }
}
