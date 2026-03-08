use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: String,
    pub token_id: String,
    pub side: String,
    pub price: String,
    pub size: String,
    pub cost: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketMetadata {
    pub token_id: String,
    pub slug: Option<String>,
    pub question: Option<String>,
    pub outcome: Option<String>,
}

const DEFAULT_STARTING_BALANCE: &str = "1000.00";

pub struct DryRunDb {
    conn: Connection,
}

fn db_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("could not determine home directory")?;
    Ok(home.join(".polymarket").join("dry-run.db"))
}

fn row_to_trade(row: &rusqlite::Row) -> rusqlite::Result<Trade> {
    Ok(Trade {
        id: row.get(0)?,
        token_id: row.get(1)?,
        side: row.get(2)?,
        price: row.get(3)?,
        size: row.get(4)?,
        cost: row.get(5)?,
        timestamp: row.get(6)?,
    })
}

impl DryRunDb {
    /// Open an in-memory database (for testing).
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.ensure_schema()?;
        Ok(db)
    }

    pub fn open() -> Result<Self> {
        let path = db_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }
        let conn = Connection::open(&path)
            .with_context(|| format!("failed to open database at {}", path.display()))?;
        let db = Self { conn };
        db.ensure_schema()?;
        Ok(db)
    }

    fn ensure_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS trades (
                id        TEXT PRIMARY KEY,
                token_id  TEXT NOT NULL,
                side      TEXT NOT NULL,
                price     TEXT NOT NULL,
                size      TEXT NOT NULL,
                cost      TEXT NOT NULL,
                timestamp TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS state (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS market_metadata (
                token_id TEXT PRIMARY KEY,
                slug     TEXT,
                question TEXT,
                outcome  TEXT
            );",
        )?;

        let has_balance: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM state WHERE key = 'balance')",
            [],
            |row| row.get(0),
        )?;

        if !has_balance {
            self.conn.execute(
                "INSERT INTO state (key, value) VALUES ('starting_balance', ?1)",
                params![DEFAULT_STARTING_BALANCE],
            )?;
            self.conn.execute(
                "INSERT INTO state (key, value) VALUES ('balance', ?1)",
                params![DEFAULT_STARTING_BALANCE],
            )?;
        }

        Ok(())
    }

    pub fn get_balance(&self) -> Result<String> {
        let balance: String =
            self.conn
                .query_row("SELECT value FROM state WHERE key = 'balance'", [], |row| {
                    row.get(0)
                })?;
        Ok(balance)
    }

    pub fn get_starting_balance(&self) -> Result<String> {
        let balance: String = self.conn.query_row(
            "SELECT value FROM state WHERE key = 'starting_balance'",
            [],
            |row| row.get(0),
        )?;
        Ok(balance)
    }

    pub fn update_balance(&self, new_balance: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE state SET value = ?1 WHERE key = 'balance'",
            params![new_balance],
        )?;
        Ok(())
    }

    pub fn insert_trade(&self, trade: &Trade) -> Result<()> {
        self.conn.execute(
            "INSERT INTO trades (id, token_id, side, price, size, cost, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                trade.id,
                trade.token_id,
                trade.side,
                trade.price,
                trade.size,
                trade.cost,
                trade.timestamp,
            ],
        )?;
        Ok(())
    }

    pub fn delete_trade(&self, trade_id: &str) -> Result<Option<Trade>> {
        let trade = self.get_trade(trade_id)?;
        if trade.is_some() {
            self.conn
                .execute("DELETE FROM trades WHERE id = ?1", params![trade_id])?;
        }
        Ok(trade)
    }

    pub fn get_trade(&self, trade_id: &str) -> Result<Option<Trade>> {
        let trade = self
            .conn
            .query_row(
                "SELECT id, token_id, side, price, size, cost, timestamp
                 FROM trades WHERE id = ?1",
                params![trade_id],
                row_to_trade,
            )
            .optional()?;
        Ok(trade)
    }

    pub fn list_trades(&self, limit: usize) -> Result<Vec<Trade>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, token_id, side, price, size, cost, timestamp
             FROM trades ORDER BY timestamp DESC LIMIT ?1",
        )?;
        let trades = stmt
            .query_map(params![limit], row_to_trade)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(trades)
    }

    pub fn all_trades(&self) -> Result<Vec<Trade>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, token_id, side, price, size, cost, timestamp
             FROM trades ORDER BY timestamp DESC",
        )?;
        let trades = stmt
            .query_map([], row_to_trade)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(trades)
    }

    /// Get the net position size for a specific token_id.
    /// Returns the sum of buy sizes minus sell sizes.
    pub fn net_position_size(&self, token_id: &str) -> Result<Decimal> {
        let mut stmt = self
            .conn
            .prepare("SELECT side, size FROM trades WHERE token_id = ?1")?;
        let rows = stmt.query_map(params![token_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut net = Decimal::ZERO;
        for row in rows {
            let (side, size_str) = row?;
            let size = Decimal::from_str(&size_str)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            match side.as_str() {
                "buy" => net += size,
                "sell" => net -= size,
                _ => {}
            }
        }
        Ok(net)
    }

    pub fn upsert_metadata(
        &self,
        token_id: &str,
        slug: Option<&str>,
        question: Option<&str>,
        outcome: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO market_metadata (token_id, slug, question, outcome)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(token_id) DO UPDATE SET
                slug = excluded.slug,
                question = excluded.question,
                outcome = excluded.outcome",
            params![token_id, slug, question, outcome],
        )?;
        Ok(())
    }

    #[allow(dead_code, reason = "used in tests and by future display commands")]
    pub fn get_metadata(&self, token_id: &str) -> Result<Option<MarketMetadata>> {
        let meta = self
            .conn
            .query_row(
                "SELECT token_id, slug, question, outcome FROM market_metadata WHERE token_id = ?1",
                params![token_id],
                |row| {
                    Ok(MarketMetadata {
                        token_id: row.get(0)?,
                        slug: row.get(1)?,
                        question: row.get(2)?,
                        outcome: row.get(3)?,
                    })
                },
            )
            .optional()?;
        Ok(meta)
    }

    pub fn all_metadata(&self) -> Result<HashMap<String, MarketMetadata>> {
        let mut stmt = self
            .conn
            .prepare("SELECT token_id, slug, question, outcome FROM market_metadata")?;
        let rows = stmt.query_map([], |row| {
            Ok(MarketMetadata {
                token_id: row.get(0)?,
                slug: row.get(1)?,
                question: row.get(2)?,
                outcome: row.get(3)?,
            })
        })?;
        let mut map = HashMap::new();
        for row in rows {
            let meta = row?;
            map.insert(meta.token_id.clone(), meta);
        }
        Ok(map)
    }

    pub fn reset(&self, starting_balance: &str) -> Result<()> {
        self.conn.execute("DELETE FROM market_metadata", [])?;
        self.conn.execute("DELETE FROM trades", [])?;
        self.conn.execute("DELETE FROM state", [])?;
        self.conn.execute(
            "INSERT INTO state (key, value) VALUES ('starting_balance', ?1)",
            params![starting_balance],
        )?;
        self.conn.execute(
            "INSERT INTO state (key, value) VALUES ('balance', ?1)",
            params![starting_balance],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn make_trade(id: &str, token_id: &str, side: &str, price: &str, size: &str) -> Trade {
        let cost = {
            let p: f64 = price.parse().unwrap();
            let s: f64 = size.parse().unwrap();
            format!("{:.2}", p * s)
        };
        Trade {
            id: id.to_string(),
            token_id: token_id.to_string(),
            side: side.to_string(),
            price: price.to_string(),
            size: size.to_string(),
            cost,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn schema_sets_default_balance() {
        let db = DryRunDb::open_in_memory().unwrap();
        assert_eq!(db.get_balance().unwrap(), DEFAULT_STARTING_BALANCE);
        assert_eq!(db.get_starting_balance().unwrap(), DEFAULT_STARTING_BALANCE);
    }

    #[test]
    fn balance_update_round_trip() {
        let db = DryRunDb::open_in_memory().unwrap();
        db.update_balance("500.50").unwrap();
        assert_eq!(db.get_balance().unwrap(), "500.50");
        // Starting balance should be unchanged
        assert_eq!(db.get_starting_balance().unwrap(), DEFAULT_STARTING_BALANCE);
    }

    #[test]
    fn insert_and_get_trade() {
        let db = DryRunDb::open_in_memory().unwrap();
        let trade = make_trade("t1", "token_abc", "buy", "0.50", "10");
        db.insert_trade(&trade).unwrap();

        let fetched = db.get_trade("t1").unwrap().expect("trade should exist");
        assert_eq!(fetched.id, "t1");
        assert_eq!(fetched.token_id, "token_abc");
        assert_eq!(fetched.side, "buy");
        assert_eq!(fetched.size, "10");
    }

    #[test]
    fn get_nonexistent_trade_returns_none() {
        let db = DryRunDb::open_in_memory().unwrap();
        assert!(db.get_trade("nonexistent").unwrap().is_none());
    }

    #[test]
    fn delete_trade_returns_it_and_removes() {
        let db = DryRunDb::open_in_memory().unwrap();
        let trade = make_trade("t1", "token_abc", "buy", "0.50", "10");
        db.insert_trade(&trade).unwrap();

        let deleted = db.delete_trade("t1").unwrap().expect("should return trade");
        assert_eq!(deleted.id, "t1");
        assert!(db.get_trade("t1").unwrap().is_none());
    }

    #[test]
    fn delete_nonexistent_returns_none() {
        let db = DryRunDb::open_in_memory().unwrap();
        assert!(db.delete_trade("nope").unwrap().is_none());
    }

    #[test]
    fn list_trades_respects_limit_and_ordering() {
        let db = DryRunDb::open_in_memory().unwrap();
        // Insert trades with different timestamps
        for i in 0..5 {
            let mut trade = make_trade(&format!("t{i}"), "tok", "buy", "0.50", "1");
            trade.timestamp = format!("2026-01-01T00:00:0{i}Z");
            db.insert_trade(&trade).unwrap();
        }

        let trades = db.list_trades(3).unwrap();
        assert_eq!(trades.len(), 3);
        // Should be newest first (DESC)
        assert_eq!(trades[0].id, "t4");
        assert_eq!(trades[1].id, "t3");
        assert_eq!(trades[2].id, "t2");
    }

    #[test]
    fn all_trades_returns_everything() {
        let db = DryRunDb::open_in_memory().unwrap();
        for i in 0..10 {
            let trade = make_trade(&format!("t{i}"), "tok", "buy", "0.50", "1");
            db.insert_trade(&trade).unwrap();
        }
        assert_eq!(db.all_trades().unwrap().len(), 10);
    }

    #[test]
    fn net_position_size_buys_only() {
        let db = DryRunDb::open_in_memory().unwrap();
        db.insert_trade(&make_trade("t1", "tok_a", "buy", "0.50", "10"))
            .unwrap();
        db.insert_trade(&make_trade("t2", "tok_a", "buy", "0.60", "5"))
            .unwrap();
        assert_eq!(
            db.net_position_size("tok_a").unwrap(),
            Decimal::from_str("15").unwrap()
        );
    }

    #[test]
    fn net_position_size_buys_and_sells() {
        let db = DryRunDb::open_in_memory().unwrap();
        db.insert_trade(&make_trade("t1", "tok_a", "buy", "0.50", "10"))
            .unwrap();
        db.insert_trade(&make_trade("t2", "tok_a", "sell", "0.60", "3"))
            .unwrap();
        assert_eq!(
            db.net_position_size("tok_a").unwrap(),
            Decimal::from_str("7").unwrap()
        );
    }

    #[test]
    fn net_position_size_unknown_token_is_zero() {
        let db = DryRunDb::open_in_memory().unwrap();
        assert_eq!(db.net_position_size("nonexistent").unwrap(), Decimal::ZERO);
    }

    #[test]
    fn net_position_size_ignores_other_tokens() {
        let db = DryRunDb::open_in_memory().unwrap();
        db.insert_trade(&make_trade("t1", "tok_a", "buy", "0.50", "10"))
            .unwrap();
        db.insert_trade(&make_trade("t2", "tok_b", "buy", "0.70", "20"))
            .unwrap();
        assert_eq!(
            db.net_position_size("tok_a").unwrap(),
            Decimal::from_str("10").unwrap()
        );
        assert_eq!(
            db.net_position_size("tok_b").unwrap(),
            Decimal::from_str("20").unwrap()
        );
    }

    #[test]
    fn reset_clears_trades_and_sets_balance() {
        let db = DryRunDb::open_in_memory().unwrap();
        db.insert_trade(&make_trade("t1", "tok", "buy", "0.50", "10"))
            .unwrap();
        db.update_balance("500.00").unwrap();

        db.reset("2000.00").unwrap();

        assert_eq!(db.all_trades().unwrap().len(), 0);
        assert_eq!(db.get_balance().unwrap(), "2000.00");
        assert_eq!(db.get_starting_balance().unwrap(), "2000.00");
    }

    #[test]
    fn store_and_retrieve_metadata() {
        let db = DryRunDb::open_in_memory().unwrap();
        db.upsert_metadata(
            "tok_a",
            Some("inflation-2026"),
            Some("Will inflation exceed 3%?"),
            Some("Yes"),
        )
        .unwrap();
        let meta = db.get_metadata("tok_a").unwrap().unwrap();
        assert_eq!(meta.slug.as_deref(), Some("inflation-2026"));
        assert_eq!(meta.question.as_deref(), Some("Will inflation exceed 3%?"));
        assert_eq!(meta.outcome.as_deref(), Some("Yes"));
    }

    #[test]
    fn get_metadata_missing_returns_none() {
        let db = DryRunDb::open_in_memory().unwrap();
        assert!(db.get_metadata("nonexistent").unwrap().is_none());
    }

    #[test]
    fn upsert_metadata_updates_existing() {
        let db = DryRunDb::open_in_memory().unwrap();
        db.upsert_metadata("tok_a", Some("old-slug"), Some("Old question"), Some("Yes"))
            .unwrap();
        db.upsert_metadata("tok_a", Some("new-slug"), Some("New question"), Some("No"))
            .unwrap();
        let meta = db.get_metadata("tok_a").unwrap().unwrap();
        assert_eq!(meta.slug.as_deref(), Some("new-slug"));
        assert_eq!(meta.question.as_deref(), Some("New question"));
        assert_eq!(meta.outcome.as_deref(), Some("No"));
    }

    #[test]
    fn reset_clears_metadata() {
        let db = DryRunDb::open_in_memory().unwrap();
        db.upsert_metadata("tok_a", Some("slug"), Some("question"), Some("Yes"))
            .unwrap();
        db.reset("1000.00").unwrap();
        assert!(db.get_metadata("tok_a").unwrap().is_none());
    }

    #[test]
    fn all_metadata_returns_map() {
        let db = DryRunDb::open_in_memory().unwrap();
        db.upsert_metadata("tok_a", Some("slug-a"), Some("Question A"), Some("Yes"))
            .unwrap();
        db.upsert_metadata("tok_b", Some("slug-b"), Some("Question B"), Some("No"))
            .unwrap();
        let map = db.all_metadata().unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(map["tok_a"].slug.as_deref(), Some("slug-a"));
        assert_eq!(map["tok_b"].slug.as_deref(), Some("slug-b"));
    }

    #[test]
    fn ensure_schema_is_idempotent() {
        let db = DryRunDb::open_in_memory().unwrap();
        db.insert_trade(&make_trade("t1", "tok", "buy", "0.50", "10"))
            .unwrap();
        db.update_balance("500.00").unwrap();

        // Running ensure_schema again should not reset anything
        db.ensure_schema().unwrap();

        assert_eq!(db.get_balance().unwrap(), "500.00");
        assert_eq!(db.all_trades().unwrap().len(), 1);
    }
}
