//! Worker definitions persisted by the desktop app.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerDefinition {
    pub id: String,
    pub name: String,
    /// Domain key in rules-tree, e.g. `git`.
    pub maintenance_domain: String,
    #[serde(default)]
    pub model_override: Option<String>,
    #[serde(default)]
    pub ollama_host: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub tasks: Vec<WorkerTask>,
    /// Partial guardrails object merged into domain defaults.
    #[serde(default)]
    pub guardrail_overrides: Option<serde_json::Value>,
    /// Host path for context JSON (managed by app; mounted into container).
    #[serde(default)]
    pub context_path: Option<String>,
    /// Long-term Docker volume name or bind path hint.
    #[serde(default)]
    pub long_term_volume: Option<String>,
    /// Agent container image (default `local-ai-worker-agent:latest` if empty).
    #[serde(default)]
    pub docker_image: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerTask {
    pub id: String,
    pub title: String,
    pub schedule: TaskSchedule,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TaskSchedule {
    OneShot,
    Cadence {
        /// Seconds between runs (simple v1 cadence).
        interval_seconds: u64,
    },
}

impl WorkerDefinition {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() {
            return Err("worker id empty".into());
        }
        if self.maintenance_domain.is_empty() {
            return Err("maintenanceDomain required".into());
        }
        if self.maintenance_domain == "git" {
            if let Some(ov) = &self.guardrail_overrides {
                if let Some(scope) = ov.get("scope") {
                    if scope.get("enforceRepositoryAllowlist") == Some(&serde_json::json!(true)) {
                        let allowed = scope
                            .get("allowedRepositories")
                            .and_then(|v| v.as_array())
                            .map(|a| !a.is_empty())
                            .unwrap_or(false);
                        if !allowed {
                            return Err(
                                "git: enforceRepositoryAllowlist is true but allowedRepositories is empty"
                                    .into(),
                            );
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
