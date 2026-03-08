use std::collections::HashMap;
use std::str::FromStr;

use anyhow::Result;
use rust_decimal::Decimal;
use serde::Serialize;

use super::db::Trade;

#[derive(Debug, Clone, Serialize)]
pub struct Position {
    pub token_id: String,
    pub net_size: String,
    pub side: String,
    pub avg_price: String,
    pub total_cost: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PositionPnl {
    pub token_id: String,
    pub side: String,
    pub size: String,
    pub avg_price: String,
    pub current_price: String,
    pub unrealized_pnl: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PnlReport {
    pub starting_balance: String,
    pub current_balance: String,
    pub positions: Vec<PositionPnl>,
    pub total_unrealized_pnl: String,
    pub position_value: String,
    pub total_value: String,
    pub total_pnl: String,
}

/// Aggregate trades into net positions per token_id.
pub fn compute_positions(trades: &[Trade]) -> Result<Vec<Position>> {
    // Accumulate net_size and total_cost per token_id
    let mut sizes: HashMap<String, Decimal> = HashMap::new();
    let mut costs: HashMap<String, Decimal> = HashMap::new();

    for trade in trades {
        let size = Decimal::from_str(&trade.size)?;
        let cost = Decimal::from_str(&trade.cost)?;

        let entry_size = sizes.entry(trade.token_id.clone()).or_insert(Decimal::ZERO);
        let entry_cost = costs.entry(trade.token_id.clone()).or_insert(Decimal::ZERO);

        match trade.side.as_str() {
            "buy" => {
                *entry_size += size;
                *entry_cost += cost;
            }
            "sell" => {
                *entry_size -= size;
                *entry_cost -= cost;
            }
            _ => {}
        }
    }

    let mut positions = Vec::new();
    for (token_id, net_size) in &sizes {
        if *net_size == Decimal::ZERO {
            continue;
        }

        let total_cost = costs.get(token_id).copied().unwrap_or(Decimal::ZERO);
        let abs_net = net_size.abs();
        let avg_price = if abs_net > Decimal::ZERO {
            (total_cost / *net_size).abs()
        } else {
            Decimal::ZERO
        };

        let side = if *net_size > Decimal::ZERO {
            "long"
        } else {
            "short"
        };

        positions.push(Position {
            token_id: token_id.clone(),
            net_size: abs_net.to_string(),
            side: side.to_string(),
            avg_price: avg_price.to_string(),
            total_cost: total_cost.abs().round_dp(2).to_string(),
        });
    }

    positions.sort_by(|a, b| a.token_id.cmp(&b.token_id));

    Ok(positions)
}

/// Compute P&L report given positions and current market prices.
pub fn compute_pnl(
    positions: &[Position],
    current_prices: &HashMap<String, Decimal>,
    starting_balance: &str,
    current_balance: &str,
) -> Result<PnlReport> {
    let starting = Decimal::from_str(starting_balance)?;
    let current = Decimal::from_str(current_balance)?;

    let mut pnl_positions = Vec::new();
    let mut total_unrealized = Decimal::ZERO;
    let mut total_position_value = Decimal::ZERO;

    for pos in positions {
        let size = Decimal::from_str(&pos.net_size)?;
        let avg = Decimal::from_str(&pos.avg_price)?;
        let current_price = current_prices.get(&pos.token_id).copied().unwrap_or(avg);

        let unrealized = match pos.side.as_str() {
            "long" => (current_price - avg) * size,
            "short" => (avg - current_price) * size,
            _ => Decimal::ZERO,
        };

        let value = current_price * size;
        total_unrealized += unrealized;
        total_position_value += value;

        pnl_positions.push(PositionPnl {
            token_id: pos.token_id.clone(),
            side: pos.side.clone(),
            size: pos.net_size.clone(),
            avg_price: pos.avg_price.clone(),
            current_price: current_price.to_string(),
            unrealized_pnl: unrealized.to_string(),
            value: value.to_string(),
        });
    }

    let total_value = current + total_position_value;
    let total_pnl = total_value - starting;

    Ok(PnlReport {
        starting_balance: starting_balance.to_string(),
        current_balance: current_balance.to_string(),
        positions: pnl_positions,
        total_unrealized_pnl: total_unrealized.to_string(),
        position_value: total_position_value.to_string(),
        total_value: total_value.to_string(),
        total_pnl: total_pnl.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_trade(token_id: &str, side: &str, price: &str, size: &str) -> Trade {
        let p = Decimal::from_str(price).unwrap();
        let s = Decimal::from_str(size).unwrap();
        Trade {
            id: format!("{token_id}-{side}"),
            token_id: token_id.to_string(),
            side: side.to_string(),
            price: price.to_string(),
            size: size.to_string(),
            cost: (p * s).to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    // ── compute_positions ──

    #[test]
    fn positions_empty_trades() {
        let positions = compute_positions(&[]).unwrap();
        assert!(positions.is_empty());
    }

    #[test]
    fn positions_single_buy() {
        let trades = vec![make_trade("tok_a", "buy", "0.50", "10")];
        let positions = compute_positions(&trades).unwrap();

        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].token_id, "tok_a");
        assert_eq!(positions[0].side, "long");
        assert_eq!(positions[0].net_size, "10");
        assert_eq!(positions[0].avg_price, "0.50");
    }

    #[test]
    fn positions_multiple_buys_same_token() {
        let trades = vec![
            make_trade("tok_a", "buy", "0.40", "10"),
            make_trade("tok_a", "buy", "0.60", "10"),
        ];
        let positions = compute_positions(&trades).unwrap();

        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].net_size, "20");
        // avg_price = total_cost / net_size = (4 + 6) / 20 = 0.50
        assert_eq!(positions[0].avg_price, "0.50");
    }

    #[test]
    fn positions_buy_and_partial_sell() {
        let trades = vec![
            make_trade("tok_a", "buy", "0.50", "10"),
            make_trade("tok_a", "sell", "0.60", "4"),
        ];
        let positions = compute_positions(&trades).unwrap();

        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].side, "long");
        assert_eq!(positions[0].net_size, "6");
    }

    #[test]
    fn positions_full_close_excluded() {
        let trades = vec![
            make_trade("tok_a", "buy", "0.50", "10"),
            make_trade("tok_a", "sell", "0.60", "10"),
        ];
        let positions = compute_positions(&trades).unwrap();
        // Fully closed position should not appear
        assert!(positions.is_empty());
    }

    #[test]
    fn positions_multiple_tokens_sorted() {
        let trades = vec![
            make_trade("tok_b", "buy", "0.30", "5"),
            make_trade("tok_a", "buy", "0.50", "10"),
        ];
        let positions = compute_positions(&trades).unwrap();

        assert_eq!(positions.len(), 2);
        // Should be sorted by token_id
        assert_eq!(positions[0].token_id, "tok_a");
        assert_eq!(positions[1].token_id, "tok_b");
    }

    #[test]
    fn positions_net_short() {
        let trades = vec![
            make_trade("tok_a", "buy", "0.50", "5"),
            make_trade("tok_a", "sell", "0.60", "10"),
        ];
        let positions = compute_positions(&trades).unwrap();

        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].side, "short");
        assert_eq!(positions[0].net_size, "5");
    }

    // ── compute_pnl ──

    #[test]
    fn pnl_no_positions() {
        let prices = HashMap::new();
        let report = compute_pnl(&[], &prices, "1000.00", "1000.00").unwrap();

        assert_eq!(report.total_unrealized_pnl, "0");
        assert_eq!(report.total_pnl, "0.00");
        assert!(report.positions.is_empty());
    }

    #[test]
    fn pnl_long_position_price_up() {
        let positions = vec![Position {
            token_id: "tok_a".to_string(),
            net_size: "10".to_string(),
            side: "long".to_string(),
            avg_price: "0.50".to_string(),
            total_cost: "5.00".to_string(),
        }];
        let mut prices = HashMap::new();
        prices.insert("tok_a".to_string(), Decimal::from_str("0.70").unwrap());

        // Spent 5.00 buying, so balance is 995.00
        let report = compute_pnl(&positions, &prices, "1000.00", "995.00").unwrap();

        // Unrealized: (0.70 - 0.50) * 10 = 2.00
        assert_eq!(report.total_unrealized_pnl, "2.00");
        // position_value = 0.70 * 10 = 7.00, total_value = 995 + 7 = 1002
        // total_pnl = 1002 - 1000 = 2.00
        assert_eq!(report.total_pnl, "2.00");
    }

    #[test]
    fn pnl_long_position_price_down() {
        let positions = vec![Position {
            token_id: "tok_a".to_string(),
            net_size: "10".to_string(),
            side: "long".to_string(),
            avg_price: "0.50".to_string(),
            total_cost: "5.00".to_string(),
        }];
        let mut prices = HashMap::new();
        prices.insert("tok_a".to_string(), Decimal::from_str("0.30").unwrap());

        let report = compute_pnl(&positions, &prices, "1000.00", "995.00").unwrap();

        // Unrealized: (0.30 - 0.50) * 10 = -2.00
        assert_eq!(report.total_unrealized_pnl, "-2.00");
    }

    #[test]
    fn pnl_missing_price_uses_avg() {
        let positions = vec![Position {
            token_id: "tok_a".to_string(),
            net_size: "10".to_string(),
            side: "long".to_string(),
            avg_price: "0.50".to_string(),
            total_cost: "5.00".to_string(),
        }];
        // No price provided for tok_a
        let prices = HashMap::new();

        let report = compute_pnl(&positions, &prices, "1000.00", "995.00").unwrap();

        // Falls back to avg_price, so unrealized = 0
        assert_eq!(report.total_unrealized_pnl, "0");
        assert_eq!(report.positions[0].current_price, "0.50");
        // position_value = 0.50 * 10 = 5.00, total_value = 995 + 5 = 1000
        assert_eq!(report.total_pnl, "0.00");
    }

    #[test]
    fn pnl_mixed_portfolio() {
        let positions = vec![
            Position {
                token_id: "tok_a".to_string(),
                net_size: "10".to_string(),
                side: "long".to_string(),
                avg_price: "0.40".to_string(),
                total_cost: "4.00".to_string(),
            },
            Position {
                token_id: "tok_b".to_string(),
                net_size: "5".to_string(),
                side: "long".to_string(),
                avg_price: "0.80".to_string(),
                total_cost: "4.00".to_string(),
            },
        ];
        let mut prices = HashMap::new();
        prices.insert("tok_a".to_string(), Decimal::from_str("0.50").unwrap());
        prices.insert("tok_b".to_string(), Decimal::from_str("0.60").unwrap());

        let report = compute_pnl(&positions, &prices, "1000.00", "992.00").unwrap();

        // tok_a: (0.50 - 0.40) * 10 = 1.00
        // tok_b: (0.60 - 0.80) * 5 = -1.00
        // Total unrealized: 0.00
        assert_eq!(report.total_unrealized_pnl, "0.00");
        // position_value = 0.50*10 + 0.60*5 = 8.00, total_value = 992+8 = 1000
        assert_eq!(report.total_pnl, "0.00");
    }

    #[test]
    fn pnl_position_value_and_total_value() {
        let positions = vec![Position {
            token_id: "tok_a".to_string(),
            net_size: "100".to_string(),
            side: "long".to_string(),
            avg_price: "0.50".to_string(),
            total_cost: "50.00".to_string(),
        }];
        let mut prices = HashMap::new();
        prices.insert("tok_a".to_string(), Decimal::from_str("0.55").unwrap());

        let report = compute_pnl(&positions, &prices, "1000.00", "950.00").unwrap();

        // position_value = 0.55 * 100 = 55.00
        assert_eq!(report.position_value, "55.00");
        // total_value = 950 + 55 = 1005.00
        assert_eq!(report.total_value, "1005.00");
        // net_pnl = 1005 - 1000 = 5.00
        assert_eq!(report.total_pnl, "5.00");
    }

    #[test]
    fn pnl_no_positions_total_value_equals_cash() {
        let report = compute_pnl(&[], &HashMap::new(), "1000.00", "1000.00").unwrap();
        assert_eq!(report.position_value, "0");
        assert_eq!(report.total_value, "1000.00");
        assert_eq!(report.total_pnl, "0.00");
    }

    #[test]
    fn pnl_deployed_capital_not_counted_as_loss() {
        let positions = vec![Position {
            token_id: "tok_a".to_string(),
            net_size: "456.25".to_string(),
            side: "long".to_string(),
            avg_price: "0.80".to_string(),
            total_cost: "365.00".to_string(),
        }];
        let mut prices = HashMap::new();
        prices.insert("tok_a".to_string(), Decimal::from_str("0.80").unwrap());

        let report = compute_pnl(&positions, &prices, "1000.00", "635.00").unwrap();

        // position_value = 0.80 * 456.25 = 365.00
        // total_value = 635 + 365 = 1000
        // net_pnl = 1000 - 1000 = 0 (NOT -365!)
        assert_eq!(report.total_pnl, "0.0000");
    }
}
