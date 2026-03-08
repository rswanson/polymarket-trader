use anyhow::{Context, Result, bail};
use polymarket_client_sdk::gamma;
use polymarket_client_sdk::gamma::types::request::{MarketBySlugRequest, MarketsRequest};
use polymarket_client_sdk::gamma::types::response::Market;
use polymarket_client_sdk::types::U256;
use std::str::FromStr;

/// Metadata about a resolved market, for display purposes.
#[derive(Debug, Clone)]
#[allow(dead_code, reason = "fields consumed by later tasks (5-9)")]
pub struct ResolvedMarket {
    pub token_id: U256,
    pub token_id_str: String,
    pub slug: Option<String>,
    pub question: Option<String>,
    pub outcome: Option<String>,
}

/// Check if a string looks like a raw U256 token ID (all digits).
pub fn is_raw_token_id(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// Find the outcome index in the outcomes list.
pub fn find_outcome_index(outcomes: &[String], outcome_name: Option<&str>) -> Result<usize> {
    match outcome_name {
        Some(name) => outcomes
            .iter()
            .position(|o| o.eq_ignore_ascii_case(name))
            .ok_or_else(|| {
                let available = outcomes.join(", ");
                anyhow::anyhow!(
                    "Outcome '{}' not found. Available outcomes: {}",
                    name,
                    available
                )
            }),
        None => {
            if outcomes.len() <= 2 {
                Ok(0)
            } else {
                let available = outcomes.join(", ");
                bail!(
                    "Market has {} outcomes — use --outcome to select one. Available: {}",
                    outcomes.len(),
                    available
                );
            }
        }
    }
}

/// Resolve a market identifier (slug or raw token ID) to a token ID and metadata.
pub async fn resolve_market(
    gamma_client: &gamma::Client,
    market: &str,
    outcome: Option<&str>,
) -> Result<ResolvedMarket> {
    if is_raw_token_id(market) {
        let token_id = U256::from_str(market).context("Failed to parse token ID as U256")?;

        // Best-effort reverse lookup for metadata
        match reverse_lookup_metadata(gamma_client, token_id).await {
            Ok(gamma_market) => {
                let outcome_name = find_outcome_for_token(token_id, &gamma_market);
                Ok(ResolvedMarket {
                    token_id,
                    token_id_str: token_id.to_string(),
                    slug: gamma_market.slug.clone(),
                    question: gamma_market.question.clone(),
                    outcome: outcome_name,
                })
            }
            Err(e) => {
                tracing::debug!("Reverse lookup failed for token {token_id}: {e}");
                Ok(ResolvedMarket {
                    token_id,
                    token_id_str: token_id.to_string(),
                    slug: None,
                    question: None,
                    outcome: None,
                })
            }
        }
    } else {
        let request = MarketBySlugRequest::builder().slug(market).build();
        let gamma_market = gamma_client
            .market_by_slug(&request)
            .await
            .with_context(|| format!("Failed to look up market by slug '{market}'"))?;
        resolve_from_gamma_market(&gamma_market, outcome)
    }
}

/// Extract token ID and metadata from a Gamma Market struct given an outcome selection.
pub fn resolve_from_gamma_market(market: &Market, outcome: Option<&str>) -> Result<ResolvedMarket> {
    let outcomes = market
        .outcomes
        .as_ref()
        .context("Market has no outcomes defined")?;
    let clob_token_ids = market
        .clob_token_ids
        .as_ref()
        .context("Market has no CLOB token IDs")?;

    let idx = find_outcome_index(outcomes, outcome)?;

    let token_id = *clob_token_ids
        .get(idx)
        .context("Outcome index out of range for CLOB token IDs")?;

    Ok(ResolvedMarket {
        token_id,
        token_id_str: token_id.to_string(),
        slug: market.slug.clone(),
        question: market.question.clone(),
        outcome: outcomes.get(idx).cloned(),
    })
}

/// Query gamma with clob_token_ids filter, returns first result.
async fn reverse_lookup_metadata(gamma_client: &gamma::Client, token_id: U256) -> Result<Market> {
    let mut request = MarketsRequest::default();
    request.clob_token_ids = vec![token_id];
    let markets = gamma_client
        .markets(&request)
        .await
        .context("Reverse lookup by CLOB token ID failed")?;
    markets
        .into_iter()
        .next()
        .context("No market found for the given token ID")
}

/// Finds the index of token_id in market.clob_token_ids, returns the outcome name at that index.
fn find_outcome_for_token(token_id: U256, market: &Market) -> Option<String> {
    let clob_ids = market.clob_token_ids.as_ref()?;
    let outcomes = market.outcomes.as_ref()?;
    let idx = clob_ids.iter().position(|id| *id == token_id)?;
    outcomes.get(idx).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_raw_token_id_valid_u256() {
        assert!(is_raw_token_id("12345"));
        assert!(is_raw_token_id(
            "52114319501245915516055106046884209969926127482827954674443846427813813222426"
        ));
    }

    #[test]
    fn is_raw_token_id_not_slug() {
        assert!(!is_raw_token_id("inflation-2026"));
        assert!(!is_raw_token_id("trump-approval"));
        assert!(!is_raw_token_id(""));
    }

    #[test]
    fn find_outcome_index_binary_default() {
        let outcomes = vec!["Yes".to_string(), "No".to_string()];
        assert_eq!(find_outcome_index(&outcomes, None).unwrap(), 0);
    }

    #[test]
    fn find_outcome_index_explicit_match() {
        let outcomes = vec!["Yes".to_string(), "No".to_string()];
        assert_eq!(find_outcome_index(&outcomes, Some("Yes")).unwrap(), 0);
        assert_eq!(find_outcome_index(&outcomes, Some("No")).unwrap(), 1);
    }

    #[test]
    fn find_outcome_index_case_insensitive() {
        let outcomes = vec!["Yes".to_string(), "No".to_string()];
        assert_eq!(find_outcome_index(&outcomes, Some("yes")).unwrap(), 0);
        assert_eq!(find_outcome_index(&outcomes, Some("no")).unwrap(), 1);
    }

    #[test]
    fn find_outcome_index_multi_outcome_requires_flag() {
        let outcomes = vec![
            "Option A".to_string(),
            "Option B".to_string(),
            "Option C".to_string(),
        ];
        assert!(find_outcome_index(&outcomes, None).is_err());
    }

    #[test]
    fn find_outcome_index_invalid_name() {
        let outcomes = vec!["Yes".to_string(), "No".to_string()];
        assert!(find_outcome_index(&outcomes, Some("Maybe")).is_err());
    }
}
