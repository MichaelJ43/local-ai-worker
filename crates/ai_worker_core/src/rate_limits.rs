//! Rolling-window rate limiters backed by SQLite timestamps.

use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection};
use std::path::Path;

use crate::Result;

pub struct RollingCounter {
    conn: Connection,
    table: &'static str,
}

impl RollingCounter {
    fn ensure_table(conn: &Connection, table: &'static str) -> Result<()> {
        conn.execute_batch(&format!(
            r#"
            CREATE TABLE IF NOT EXISTS {table} (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                worker_id TEXT NOT NULL,
                ts TEXT NOT NULL,
                event_type TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_{table}_worker_ts ON {table}(worker_id, ts);
            "#
        ))?;
        Ok(())
    }

    pub fn open_pr_day(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::ensure_table(&conn, "rolling_pr_create")?;
        Ok(Self {
            conn,
            table: "rolling_pr_create",
        })
    }

    pub fn open_github_mutations_hour(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::ensure_table(&conn, "rolling_github_mut")?;
        Ok(Self {
            conn,
            table: "rolling_github_mut",
        })
    }

    fn prune_older_than(&self, worker_id: &str, cutoff: DateTime<Utc>) -> Result<()> {
        self.conn.execute(
            &format!(
                "DELETE FROM {} WHERE worker_id = ?1 AND ts < ?2",
                self.table
            ),
            params![worker_id, cutoff.to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn count_in_window(
        &self,
        worker_id: &str,
        window: Duration,
        now: DateTime<Utc>,
    ) -> Result<u64> {
        let cutoff = now - window;
        self.prune_older_than(worker_id, cutoff)?;
        let n: i64 = self.conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM {} WHERE worker_id = ?1 AND ts >= ?2",
                self.table
            ),
            params![worker_id, cutoff.to_rfc3339()],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }

    pub fn record_event(&self, worker_id: &str, event_type: &str, now: DateTime<Utc>) -> Result<()> {
        self.conn.execute(
            &format!(
                "INSERT INTO {} (worker_id, ts, event_type) VALUES (?1, ?2, ?3)",
                self.table
            ),
            params![worker_id, now.to_rfc3339(), event_type],
        )?;
        Ok(())
    }

    pub fn try_consume_pr_slot(
        &self,
        worker_id: &str,
        max_per_rolling_24h: u64,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        let n = self.count_in_window(worker_id, Duration::hours(24), now)?;
        if n >= max_per_rolling_24h {
            return Ok(false);
        }
        self.record_event(worker_id, "pr_create", now)?;
        Ok(true)
    }

    pub fn try_consume_github_mutation(
        &self,
        worker_id: &str,
        max_per_rolling_hour: u64,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        let n = self.count_in_window(worker_id, Duration::hours(1), now)?;
        if n >= max_per_rolling_hour {
            return Ok(false);
        }
        self.record_event(worker_id, "github_mut", now)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn rolling_24h_pr() {
        let dir = tempdir().unwrap();
        let c = RollingCounter::open_pr_day(dir.path().join("r.db")).unwrap();
        let t = Utc::now();
        assert!(c.try_consume_pr_slot("w", 2, t).unwrap());
        assert!(c.try_consume_pr_slot("w", 2, t).unwrap());
        assert!(!c.try_consume_pr_slot("w", 2, t).unwrap());
    }
}
