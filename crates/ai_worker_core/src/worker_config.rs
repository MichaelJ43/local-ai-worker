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
    /// Extra instructions merged into `system-prompt.txt` after domain rules when the Docker runtime is materialized.
    #[serde(default)]
    pub worker_prompt: Option<String>,
    /// `observe` (default): gather repo facts and model output; model-proposed commands are not executed.
    /// `apply_git`: execute model-proposed `git` argv only (guarded wrappers).
    /// `apply_github`: execute model-proposed `git` and `gh` argv (guarded wrappers).
    #[serde(default)]
    pub repo_execution_mode: Option<String>,
    /// Optional whitelisted test commands (bare names on `PATH` + argv) run from `apply_git` / `apply_github` repo cycles only.
    #[serde(default)]
    pub allowed_test_profiles: Option<Vec<AllowedTestProfile>>,
    /// Optional `docker run --network <name>` (Docker-level isolation; see docs/REPO_AGENT_SANDBOX.md).
    #[serde(default)]
    pub docker_network: Option<String>,
    /// Repo tree patch / diff policy for the experimental repo agent.
    #[serde(default)]
    pub repo_sandbox_policy: Option<RepoSandboxPolicy>,
}

/// Narrow test-runner profile for repo autonomous workers (`apply_git`+ only).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllowedTestProfile {
    /// Bare executable resolved via container `PATH` (no slashes or path segments).
    pub command: String,
    #[serde(default)]
    pub argv: Vec<String>,
}

/// Experimental repo-agent sandbox knobs (materialized into `agent-config.json` as `sandboxPolicy`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RepoSandboxPolicy {
    /// When true with `apply_git` / `apply_github`, applies `fileWrites` from structured model JSON.
    #[serde(default)]
    pub file_writes_enabled: bool,
    /// When true, applies each string in model `unifiedDiffs[]` via `git apply` (read-only git binary).
    #[serde(default)]
    pub apply_unified_diffs_enabled: bool,
    #[serde(default = "serde_default_patch_max_file_bytes")]
    pub patch_max_file_bytes: u64,
    #[serde(default = "serde_default_patch_max_total_bytes")]
    pub patch_max_total_bytes: u64,
    #[serde(default = "serde_default_patch_max_unified_diff_bytes")]
    pub patch_max_unified_diff_bytes: u64,
    /// Allow writes/deletes under `.git/` (dangerous; dummy-repo only).
    #[serde(default)]
    pub allow_git_internals_writes: bool,
}

fn serde_default_patch_max_file_bytes() -> u64 {
    384 * 1024
}

fn serde_default_patch_max_total_bytes() -> u64 {
    3 * 1024 * 1024
}

fn serde_default_patch_max_unified_diff_bytes() -> u64 {
    1024 * 1024
}

impl Default for RepoSandboxPolicy {
    fn default() -> Self {
        Self {
            file_writes_enabled: false,
            apply_unified_diffs_enabled: false,
            patch_max_file_bytes: serde_default_patch_max_file_bytes(),
            patch_max_total_bytes: serde_default_patch_max_total_bytes(),
            patch_max_unified_diff_bytes: serde_default_patch_max_unified_diff_bytes(),
            allow_git_internals_writes: false,
        }
    }
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
        #[serde(rename = "intervalSeconds", alias = "interval_seconds")]
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
        self.validate_allowed_test_profiles()?;
        self.validate_docker_network_field()?;
        self.validate_repo_sandbox_policy()?;

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

    fn validate_docker_network_field(&self) -> Result<(), String> {
        let Some(ref raw) = self.docker_network else {
            return Ok(());
        };
        let s = raw.trim();
        if s.is_empty() {
            return Ok(());
        }
        if s.chars().any(|c| {
            c.is_control()
                || matches!(
                    c,
                    ';' | '|' | '&' | '$' | '`' | '(' | ')' | '<' | '>' | '"' | '\''
                )
        }) {
            return Err(
                "dockerNetwork contains unsafe characters — use a simple docker network name only"
                    .into(),
            );
        }
        if !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            return Err(
                "dockerNetwork must be alphanumeric with - _ . only (no slashes)".into(),
            );
        }
        Ok(())
    }

    fn validate_repo_sandbox_policy(&self) -> Result<(), String> {
        let policy = match &self.repo_sandbox_policy {
            None => return Ok(()),
            Some(p) => p,
        };
        const MIN: u64 = 1024;
        const MAX: u64 = 50 * 1024 * 1024;

        fn check_limit(label: &str, v: u64) -> Result<(), String> {
            if !(MIN..=MAX).contains(&v) {
                return Err(format!(
                    "repoSandboxPolicy.{label} must be between {MIN} and {MAX} bytes",
                ));
            }
            Ok(())
        }

        check_limit("patchMaxFileBytes", policy.patch_max_file_bytes)?;
        check_limit("patchMaxTotalBytes", policy.patch_max_total_bytes)?;
        check_limit(
            "patchMaxUnifiedDiffBytes",
            policy.patch_max_unified_diff_bytes,
        )?;
        if policy.patch_max_total_bytes < policy.patch_max_file_bytes {
            return Err(
                "repoSandboxPolicy.patchMaxTotalBytes must be >= patchMaxFileBytes".into(),
            );
        }
        Ok(())
    }

    fn validate_allowed_test_profiles(&self) -> Result<(), String> {
        let Some(ref rows) = self.allowed_test_profiles else {
            return Ok(());
        };
        for (i, p) in rows.iter().enumerate() {
            let cmd = p.command.trim();
            if cmd.is_empty() {
                return Err(format!("allowedTestProfiles[{i}].command cannot be empty"));
            }
            if !bare_path_executable_name_ok(cmd) {
                return Err(format!(
                    "allowedTestProfiles[{i}].command must be a bare executable name on PATH \
                     (alphanumeric plus ._+- only; no / or \\\\)."
                ));
            }
            for (ai, arg) in p.argv.iter().enumerate() {
                if arg.as_bytes().contains(&0u8) {
                    return Err(format!(
                        "allowedTestProfiles[{i}].argv[{ai}] contains invalid bytes"
                    ));
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

fn bare_path_executable_name_ok(cmd: &str) -> bool {
    if cmd.bytes().any(|b| matches!(b, b'/' | b'\\')) {
        return false;
    }
    if cmd.contains("..") {
        return false;
    }
    cmd.chars().all(|c| c.is_ascii_alphanumeric() || "._-+".contains(c))
}

#[cfg(test)]
mod allowed_profile_tests {
    use super::*;

    fn minimal() -> WorkerDefinition {
        WorkerDefinition {
            id: "w".into(),
            name: "n".into(),
            maintenance_domain: "misc".into(),
            escalation_path: vec![],
            model_override: None,
            ollama_host: None,
            enabled: false,
            tasks: vec![],
            guardrail_overrides: None,
            context_path: None,
            long_term_volume: None,
            docker_image: None,
            env_from_secrets: vec![],
            hybrid_options: None,
            worker_prompt: None,
            repo_execution_mode: None,
            allowed_test_profiles: None,
            docker_network: None,
            repo_sandbox_policy: None,
        }
    }

    #[test]
    fn rejects_slash_in_test_command() {
        let mut w = minimal();
        w.allowed_test_profiles = Some(vec![AllowedTestProfile {
            command: "bin/sh".into(),
            argv: vec![],
        }]);
        assert!(w.validate().is_err());
    }

    #[test]
    fn rejects_illegal_docker_network_value() {
        let mut w = minimal();
        w.docker_network = Some("evil;curl".into());
        assert!(w.validate().is_err());
    }

    #[test]
    fn allows_empty_profiles() {
        let w = minimal();
        assert!(w.validate().is_ok());
    }
}
