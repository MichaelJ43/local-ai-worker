//! Worker definitions persisted by the desktop app.

use serde::{Deserialize, Serialize};

use crate::llm_source::{self, LlmSourceDefinition};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerDefinition {
    pub id: String,
    pub name: String,
    /// Domain key in rules-tree, e.g. `git`.
    pub maintenance_domain: String,
    /// Ordered list of `llm_sources.json` tier ids (OLLAMA tier required for Docker agent enablement).
    #[serde(default)]
    pub escalation_path: Vec<String>,
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
    /// Map stored secret keys (Keychain) to container env vars, e.g. `GITHUB_TOKEN` ← `github_token`.
    #[serde(default)]
    pub env_from_secrets: Vec<EnvSecretBinding>,
    /// Hybrid local Ollama + Cursor SDK escalation (host-side; desktop app orchestration).
    #[serde(default)]
    pub hybrid_options: Option<HybridOptions>,
}

/// Optional hybrid Ollama → Cursor SDK escalation parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HybridOptions {
    /// Keychain entry name storing the Cursor API token (fallback: `cursor_api_key`).
    #[serde(default)]
    pub cursor_secret_key: Option<String>,
    #[serde(default)]
    pub repo_url: Option<String>,
    #[serde(default)]
    pub starting_ref: Option<String>,
    /// Host filesystem path passed to Cursor local SDK as cwd (when escalation is enabled).
    #[serde(default)]
    pub workspace_path: Option<String>,
    /// When false, only the local Cursor agent is used (`local.cwd`); no cloud-hosted repo clone.
    #[serde(default = "serde_default_true")]
    pub allow_cloud_escalation: bool,
    #[serde(default)]
    pub local_phase_timeout_ms: Option<u64>,
    #[serde(default)]
    pub local_max_attempts: Option<u32>,
    /// Example: `composer-2` — see Cursor SDK model list when using cloud.
    #[serde(default)]
    pub cursor_model_id: Option<String>,
}

fn serde_default_true() -> bool {
    true
}

impl Default for HybridOptions {
    fn default() -> Self {
        Self {
            cursor_secret_key: None,
            repo_url: None,
            starting_ref: None,
            workspace_path: None,
            allow_cloud_escalation: true,
            local_phase_timeout_ms: None,
            local_max_attempts: None,
            cursor_model_id: None,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvSecretBinding {
    pub env_var: String,
    pub secret_key: String,
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
        for b in &self.env_from_secrets {
            if b.env_var.trim().is_empty() || b.secret_key.trim().is_empty() {
                return Err("envFromSecrets: envVar and secretKey required".into());
            }
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

    /// Validate escalation path refs + Docker enable requirements when catalogs are loaded.
    pub fn validate_with_llm_catalog(
        &self,
        sources: &[LlmSourceDefinition],
    ) -> Result<(), String> {
        for tier_id in &self.escalation_path {
            if tier_id.trim().is_empty() {
                return Err("escalationPath entries must not be empty".into());
            }
            if llm_source::source_by_id(sources, tier_id).is_none() {
                return Err(format!(
                    "escalationPath references unknown source id `{}` — check LLM sources",
                    tier_id
                ));
            }
        }
        if self.enabled {
            if self.escalation_path.is_empty() {
                return Err(
                    "enabled workers need a non-empty escalationPath (add LLM sources)".into(),
                );
            }
            let first_cursor_idx = self
                .escalation_path
                .iter()
                .position(|id| {
                    llm_source::source_by_id(sources, id)
                        .is_some_and(|s| matches!(s, LlmSourceDefinition::Cursor { .. }))
                });
            let first_oll_idx = self.escalation_path.iter().position(|id| {
                llm_source::source_by_id(sources, id)
                    .is_some_and(|s| matches!(s, LlmSourceDefinition::Ollama { .. }))
            });

            if let (Some(oi), Some(ci)) = (first_oll_idx, first_cursor_idx) {
                if ci < oi {
                    return Err(
                        "Hybrid escalation ordering not supported yet: Cursor tier before Ollama tier"
                            .into(),
                    );
                }
            }
            if llm_source::resolve_first_ollama_for_escalation(sources, &self.escalation_path)
                .is_err()
            {
                return Err(
                    "enabled Docker workers require escalationPath to include an Ollama source tier"
                        .into(),
                );
            }
        }
        Ok(())
    }
}
