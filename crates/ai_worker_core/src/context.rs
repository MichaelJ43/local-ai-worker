//! Per-worker evolving context JSON (load / save / bump version).

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerContext {
    pub version: u32,
    #[serde(default)]
    pub goals: Vec<String>,
    #[serde(default)]
    pub facts_learned: Vec<String>,
    #[serde(default)]
    pub open_questions: Vec<String>,
    #[serde(default)]
    pub last_summary: String,
    #[serde(default)]
    pub last_guardrail_block: Option<String>,
    #[serde(default)]
    pub extra: serde_json::Value,
}

impl Default for WorkerContext {
    fn default() -> Self {
        Self {
            version: 1,
            goals: vec![],
            facts_learned: vec![],
            open_questions: vec![],
            last_summary: String::new(),
            last_guardrail_block: None,
            extra: serde_json::json!({}),
        }
    }
}

impl WorkerContext {
    pub fn load_or_create(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if path.exists() {
            let bytes = std::fs::read(path)?;
            Ok(serde_json::from_slice(&bytes)?)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Append a learning note and trim to `max_facts` entries (FIFO drop from front).
    pub fn record_fact(&mut self, fact: impl Into<String>, max_facts: usize) {
        self.facts_learned.push(fact.into());
        while self.facts_learned.len() > max_facts {
            self.facts_learned.remove(0);
        }
    }

    pub fn excerpt_for_prompt(&self, max_chars: usize) -> String {
        let s = serde_json::to_string_pretty(self).unwrap_or_default();
        if s.len() <= max_chars {
            s
        } else {
            format!("{}…", &s[..max_chars])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn roundtrip_context() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("ctx.json");
        let mut c = WorkerContext::default();
        c.goals.push("keep deps fresh".into());
        c.save(&p).unwrap();
        let loaded = WorkerContext::load_or_create(&p).unwrap();
        assert_eq!(loaded.goals, c.goals);
    }
}
