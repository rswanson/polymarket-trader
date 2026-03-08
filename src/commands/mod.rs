pub mod account;
pub mod dry_run;
pub mod markets;
pub mod orders;
pub mod prices;

use anyhow::Result;
use polymarket_client_sdk::clob::types::Side;
use polymarket_client_sdk::types::U256;
use std::str::FromStr;

/// Pagination end sentinel used by the Polymarket CLOB API.
pub const CLOB_END_CURSOR: &str = "LTE=";

pub fn parse_token_id(s: &str) -> Result<U256> {
    U256::from_str(s).map_err(|e| anyhow::anyhow!("Invalid token ID: {e}"))
}

pub fn parse_side(s: &str) -> Result<Side> {
    match s.to_lowercase().as_str() {
        "buy" => Ok(Side::Buy),
        "sell" => Ok(Side::Sell),
        other => Err(anyhow::anyhow!(
            "Invalid side '{other}', expected 'buy' or 'sell'"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_side_buy_lowercase() {
        assert_eq!(parse_side("buy").unwrap(), Side::Buy);
    }

    #[test]
    fn parse_side_sell_lowercase() {
        assert_eq!(parse_side("sell").unwrap(), Side::Sell);
    }

    #[test]
    fn parse_side_case_insensitive() {
        assert_eq!(parse_side("BUY").unwrap(), Side::Buy);
        assert_eq!(parse_side("Sell").unwrap(), Side::Sell);
        assert_eq!(parse_side("sElL").unwrap(), Side::Sell);
    }

    #[test]
    fn parse_side_invalid() {
        let err = parse_side("hold").unwrap_err();
        assert!(err.to_string().contains("hold"));
        assert!(err.to_string().contains("expected 'buy' or 'sell'"));
    }

    #[test]
    fn parse_side_empty() {
        assert!(parse_side("").is_err());
    }

    #[test]
    fn parse_token_id_valid() {
        let result = parse_token_id("12345");
        assert!(result.is_ok());
    }

    #[test]
    fn parse_token_id_large_number() {
        // A realistic large token ID
        let result = parse_token_id(
            "52114319501245915516055106046884209969926127482827954674443846427813813222426",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn parse_token_id_invalid() {
        let err = parse_token_id("not_a_number").unwrap_err();
        assert!(err.to_string().contains("Invalid token ID"));
    }

    #[test]
    fn parse_token_id_empty_is_zero() {
        // Empty string parses as U256 zero
        let result = parse_token_id("");
        assert!(result.is_ok());
    }
}
