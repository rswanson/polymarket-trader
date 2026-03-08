use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
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

pub struct DryRunDb {
    conn: Connection,
}

fn db_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("could not determine home directory")?;
    Ok(home.join(".polymarket").join("dry-run.db"))
}

impl DryRunDb {
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
            );",
        )?;

        let has_balance: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM state WHERE key = 'balance')",
            [],
            |row| row.get(0),
        )?;

        if !has_balance {
            self.conn.execute(
                "INSERT INTO state (key, value) VALUES ('starting_balance', '1000.00')",
                [],
            )?;
            self.conn.execute(
                "INSERT INTO state (key, value) VALUES ('balance', '1000.00')",
                [],
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
                |row| {
                    Ok(Trade {
                        id: row.get(0)?,
                        token_id: row.get(1)?,
                        side: row.get(2)?,
                        price: row.get(3)?,
                        size: row.get(4)?,
                        cost: row.get(5)?,
                        timestamp: row.get(6)?,
                    })
                },
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
            .query_map(params![limit], |row| {
                Ok(Trade {
                    id: row.get(0)?,
                    token_id: row.get(1)?,
                    side: row.get(2)?,
                    price: row.get(3)?,
                    size: row.get(4)?,
                    cost: row.get(5)?,
                    timestamp: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(trades)
    }

    pub fn all_trades(&self) -> Result<Vec<Trade>> {
        self.list_trades(usize::MAX)
    }

    pub fn reset(&self, starting_balance: &str) -> Result<()> {
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
