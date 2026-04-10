//! SQLite audit log for GitHub mutations and git/gh failures.

use chrono::Utc;
use rusqlite::{params, Connection};
use serde_json::json;
use std::path::Path;

use crate::Result;

pub struct AuditLog {
    conn: Connection,
}

impl AuditLog {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS github_audit (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts TEXT NOT NULL,
                worker_id TEXT NOT NULL,
                operation TEXT NOT NULL,
                repo TEXT,
                http_method TEXT,
                success INTEGER NOT NULL,
                detail TEXT
            );
            CREATE TABLE IF NOT EXISTS git_cli_audit (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts TEXT NOT NULL,
                worker_id TEXT NOT NULL,
                argv_redacted TEXT NOT NULL,
                success INTEGER NOT NULL,
                stderr_excerpt TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_github_ts ON github_audit(ts);
            CREATE INDEX IF NOT EXISTS idx_git_cli_ts ON git_cli_audit(ts);
            "#,
        )?;
        Ok(Self { conn })
    }

    pub fn record_github_mutation(
        &self,
        worker_id: &str,
        operation: &str,
        repo: Option<&str>,
        http_method: Option<&str>,
        success: bool,
        detail: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO github_audit (ts, worker_id, operation, repo, http_method, success, detail) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                Utc::now().to_rfc3339(),
                worker_id,
                operation,
                repo,
                http_method,
                success as i32,
                detail,
            ],
        )?;
        Ok(())
    }

    pub fn record_git_cli(
        &self,
        worker_id: &str,
        argv_redacted: &str,
        success: bool,
        stderr_excerpt: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO git_cli_audit (ts, worker_id, argv_redacted, success, stderr_excerpt) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                Utc::now().to_rfc3339(),
                worker_id,
                argv_redacted,
                success as i32,
                stderr_excerpt,
            ],
        )?;
        Ok(())
    }

    pub fn recent_github_rows(&self, limit: usize) -> Result<Vec<(String, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT ts, worker_id, operation FROM github_audit ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map([limit as i64], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Rows for UI / API (newest first).
    pub fn recent_github_json(&self, limit: usize) -> Result<Vec<serde_json::Value>> {
        let mut stmt = self.conn.prepare(
            "SELECT ts, worker_id, operation, repo, http_method, success, detail FROM github_audit ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map([limit as i64], |r| {
                Ok(json!({
                    "ts": r.get::<_, String>(0)?,
                    "workerId": r.get::<_, String>(1)?,
                    "operation": r.get::<_, String>(2)?,
                    "repo": r.get::<_, Option<String>>(3)?,
                    "httpMethod": r.get::<_, Option<String>>(4)?,
                    "success": r.get::<_, i64>(5)? != 0,
                    "detail": r.get::<_, Option<String>>(6)?,
                }))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn insert_github() {
        let dir = tempdir().unwrap();
        let db = AuditLog::open(dir.path().join("a.db")).unwrap();
        db.record_github_mutation("w1", "pr.create", Some("o/r"), Some("POST"), true, None)
            .unwrap();
        let r = db.recent_github_rows(5).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].2, "pr.create");
    }

    #[test]
    fn recent_github_json_shape() {
        let dir = tempdir().unwrap();
        let db = AuditLog::open(dir.path().join("b.db")).unwrap();
        db.record_github_mutation("w2", "issue.open", None, None, false, Some("rate limit"))
            .unwrap();
        let j = db.recent_github_json(5).unwrap();
        assert_eq!(j.len(), 1);
        assert_eq!(j[0].get("workerId").and_then(|v| v.as_str()), Some("w2"));
        assert_eq!(j[0].get("success").and_then(|v| v.as_bool()), Some(false));
    }
}
