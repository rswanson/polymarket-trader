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
}

#[derive(Debug, Clone, Serialize)]
pub struct PnlReport {
    pub starting_balance: String,
    pub current_balance: String,
    pub positions: Vec<PositionPnl>,
    pub total_unrealized_pnl: String,
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

    for pos in positions {
        let size = Decimal::from_str(&pos.net_size)?;
        let avg = Decimal::from_str(&pos.avg_price)?;
        let current_price = current_prices.get(&pos.token_id).copied().unwrap_or(avg);

        let unrealized = match pos.side.as_str() {
            "long" => (current_price - avg) * size,
            "short" => (avg - current_price) * size,
            _ => Decimal::ZERO,
        };

        total_unrealized += unrealized;

        pnl_positions.push(PositionPnl {
            token_id: pos.token_id.clone(),
            side: pos.side.clone(),
            size: pos.net_size.clone(),
            avg_price: pos.avg_price.clone(),
            current_price: current_price.to_string(),
            unrealized_pnl: unrealized.to_string(),
        });
    }

    let total_pnl = (current - starting) + total_unrealized;

    Ok(PnlReport {
        starting_balance: starting_balance.to_string(),
        current_balance: current_balance.to_string(),
        positions: pnl_positions,
        total_unrealized_pnl: total_unrealized.to_string(),
        total_pnl: total_pnl.to_string(),
    })
}
