//! Git / `gh` invocation checks and post-run accounting (for worker-guard CLI).

use chrono::Utc;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

use crate::audit::AuditLog;
use crate::rate_limits::RollingCounter;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimits {
    #[serde(default = "default_pr_day")]
    pub max_pull_requests_created_per_day: u64,
    #[serde(default = "default_gh_mut")]
    pub max_github_api_mutations_per_hour: u64,
    #[serde(default = "default_push_gap")]
    pub min_seconds_between_git_pushes: u64,
}

impl Default for RateLimits {
    fn default() -> Self {
        Self {
            max_pull_requests_created_per_day: default_pr_day(),
            max_github_api_mutations_per_hour: default_gh_mut(),
            min_seconds_between_git_pushes: default_push_gap(),
        }
    }
}

fn default_pr_day() -> u64 {
    10
}
fn default_gh_mut() -> u64 {
    120
}
fn default_push_gap() -> u64 {
    60
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    #[serde(default)]
    pub enforce_repository_allowlist: bool,
    #[serde(default)]
    pub allowed_repositories: Vec<String>,
    #[serde(default)]
    pub denied_repositories: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchSafety {
    #[serde(default = "default_protected")]
    pub protected_default_branch_names: Vec<String>,
    #[serde(default = "default_true")]
    pub forbid_force_push_to_default_branch: bool,
    #[serde(default = "default_true")]
    pub forbid_delete_remote_branch: bool,
    #[serde(default = "default_true")]
    pub forbid_history_rewrite_on_shared_branches: bool,
}

impl Default for BranchSafety {
    fn default() -> Self {
        Self {
            protected_default_branch_names: default_protected(),
            forbid_force_push_to_default_branch: true,
            forbid_delete_remote_branch: true,
            forbid_history_rewrite_on_shared_branches: true,
        }
    }
}

fn default_protected() -> Vec<String> {
    vec!["main".into(), "master".into()]
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergePolicy {
    #[serde(default = "default_true")]
    pub allow_merging_pull_requests: bool,
    #[serde(default = "default_true")]
    pub require_ci_success_before_merge: bool,
}

impl Default for MergePolicy {
    fn default() -> Self {
        Self {
            allow_merging_pull_requests: true,
            require_ci_success_before_merge: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditPolicy {
    #[serde(default = "default_true")]
    pub record_all_github_mutations: bool,
    #[serde(default = "default_true")]
    pub record_git_cli_failures: bool,
}

impl Default for AuditPolicy {
    fn default() -> Self {
        Self {
            record_all_github_mutations: true,
            record_git_cli_failures: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretsAndLogs {
    #[serde(default = "default_true")]
    pub redact_tokens_in_audit_and_app_logs: bool,
}

impl Default for SecretsAndLogs {
    fn default() -> Self {
        Self {
            redact_tokens_in_audit_and_app_logs: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct EffectiveGitGuardrails {
    pub rate_limits: RateLimits,
    pub scope: Scope,
    pub branch_safety: BranchSafety,
    pub merge_policy: MergePolicy,
    pub audit_policy: AuditPolicy,
    pub secrets_and_logs: SecretsAndLogs,
}

impl EffectiveGitGuardrails {
    pub fn from_guardrails_value(v: &serde_json::Value) -> Self {
        let mut out = Self::default();
        let Some(obj) = v.as_object() else {
            return out;
        };
        if let Some(r) = obj.get("rateLimits") {
            if let Ok(r) = serde_json::from_value::<RateLimits>(r.clone()) {
                out.rate_limits = r;
            }
        }
        if let Some(r) = obj.get("scope") {
            if let Ok(r) = serde_json::from_value::<Scope>(r.clone()) {
                out.scope = r;
            }
        }
        if let Some(r) = obj.get("branchSafety") {
            if let Ok(r) = serde_json::from_value::<BranchSafety>(r.clone()) {
                out.branch_safety = r;
            }
        }
        if let Some(r) = obj.get("mergePolicy") {
            if let Ok(r) = serde_json::from_value::<MergePolicy>(r.clone()) {
                out.merge_policy = r;
            }
        }
        if let Some(r) = obj.get("auditPolicy") {
            if let Ok(r) = serde_json::from_value::<AuditPolicy>(r.clone()) {
                out.audit_policy = r;
            }
        }
        if let Some(r) = obj.get("secretsAndLogs") {
            if let Ok(r) = serde_json::from_value::<SecretsAndLogs>(r.clone()) {
                out.secrets_and_logs = r;
            }
        }
        out
    }

    pub fn load_path(path: impl AsRef<Path>) -> std::result::Result<Self, String> {
        let bytes = fs::read(path.as_ref()).map_err(|e| e.to_string())?;
        let v: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        Ok(Self::from_guardrails_value(&v))
    }
}

/// Optional `owner/repo` from env `AI_WORKER_REPO` (set by tooling when known).
pub fn repo_from_env() -> Option<String> {
    std::env::var("AI_WORKER_REPO")
        .ok()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
}

fn normalize_repo(s: &str) -> String {
    s.trim()
        .trim_end_matches(".git")
        .replace("git@github.com:", "github.com/")
        .replace("https://github.com/", "")
        .to_lowercase()
}

pub fn repo_allowed(gr: &EffectiveGitGuardrails, repo: Option<&str>) -> std::result::Result<(), String> {
    let Some(repo) = repo.map(normalize_repo).filter(|s| !s.is_empty()) else {
        if gr.scope.enforce_repository_allowlist {
            return Err(
                "repository allowlist enforced but AI_WORKER_REPO is not set (cannot verify scope)"
                    .into(),
            );
        }
        return Ok(());
    };
    for denied in &gr.scope.denied_repositories {
        let d = normalize_repo(denied);
        if !d.is_empty() && repo == d {
            return Err(format!("repository {repo} is denied by guardrails"));
        }
    }
    if gr.scope.enforce_repository_allowlist {
        let ok = gr
            .scope
            .allowed_repositories
            .iter()
            .any(|a| normalize_repo(a) == repo);
        if !ok {
            return Err(format!(
                "repository {repo} is not in allowedRepositories (allowlist enforced)"
            ));
        }
    }
    Ok(())
}

pub fn redact_argv_for_log(args: &[String], do_redact: bool) -> String {
    let joined = args.join(" ");
    if !do_redact {
        return joined;
    }
    let mut s = joined;
    for pat in ["GITHUB_TOKEN", "ghp_", "gho_", "ghu_", "ghs_", "xoxb-"] {
        if s.contains(pat) {
            s = s.replace(pat, "<redacted>");
        }
    }
    if let Ok(tok) = std::env::var("GITHUB_TOKEN") {
        if !tok.is_empty() && s.contains(&tok) {
            s = s.replace(&tok, "<redacted>");
        }
    }
    s
}

fn skip_git_globals(args: &[String]) -> usize {
    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "-c" || a == "--git-dir" || a == "--work-tree" {
            i = i.saturating_add(2);
            continue;
        }
        if a == "-C" {
            i = i.saturating_add(2);
            continue;
        }
        if a.starts_with('-') {
            i += 1;
            continue;
        }
        break;
    }
    i
}

pub fn git_subcommand(args: &[String]) -> Option<String> {
    let i = skip_git_globals(args);
    args.get(i).cloned()
}

pub fn git_argv_has_push(args: &[String]) -> bool {
    git_subcommand(args).as_deref() == Some("push")
}

pub fn git_argv_has_force_push(args: &[String]) -> bool {
    if !git_argv_has_push(args) {
        return false;
    }
    args.iter().any(|a| a == "--force" || a == "-f" || a == "--force-with-lease")
}

pub fn git_argv_has_delete_remote(args: &[String]) -> bool {
    git_argv_has_push(args) && args.iter().any(|a| a == "--delete")
}

pub fn git_argv_refspecs_touch_protected(args: &[String], protected: &[String]) -> bool {
    let prot: Vec<String> = protected.iter().map(|p| p.to_lowercase()).collect();
    for a in args {
        let al = a.to_lowercase();
        if prot.iter().any(|p| al.contains(p)) {
            return true;
        }
        if al.starts_with('+') && prot.iter().any(|p| al.contains(p)) {
            return true;
        }
    }
    false
}

pub fn git_precheck(args: &[String], gr: &EffectiveGitGuardrails) -> std::result::Result<(), String> {
    repo_from_env()
        .as_deref()
        .map(|r| repo_allowed(gr, Some(r)))
        .unwrap_or_else(|| repo_allowed(gr, None))?;

    let sub = git_subcommand(args);
    let Some(ref cmd) = sub else {
        return Ok(());
    };

    if cmd == "push" {
        if gr.branch_safety.forbid_delete_remote_branch && git_argv_has_delete_remote(args) {
            return Err("guardrail: deleting remote branches is forbidden".into());
        }
        if gr.branch_safety.forbid_force_push_to_default_branch
            && git_argv_has_force_push(args)
            && git_argv_refspecs_touch_protected(args, &gr.branch_safety.protected_default_branch_names)
        {
            return Err(
                "guardrail: force push to a protected default branch name is forbidden".into(),
            );
        }
        if gr.branch_safety.forbid_history_rewrite_on_shared_branches
            && args.iter().any(|a| {
                let s = a.as_str();
                s.starts_with('+') && s != "+" && !s.starts_with("++")
            })
        {
            return Err(
                "guardrail: history rewrite via '+' refspec on push is forbidden".into(),
            );
        }

        let path = push_cooldown_path();
        if gr.rate_limits.min_seconds_between_git_pushes > 0 {
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(last) = data.trim().parse::<u64>() {
                    let now = now_epoch_secs();
                    if now.saturating_sub(last) < gr.rate_limits.min_seconds_between_git_pushes {
                        return Err(format!(
                            "guardrail: minSecondsBetweenGitPushes — wait {}s",
                            gr.rate_limits.min_seconds_between_git_pushes
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

fn push_cooldown_path() -> PathBuf {
    std::env::var("AI_GUARD_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/persist"))
        .join(".ai_worker_last_push_epoch")
}

fn guard_db_path() -> PathBuf {
    std::env::var("AI_GUARD_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/persist"))
        .join("guard_state.sqlite3")
}

fn audit_db_path_opt() -> Option<PathBuf> {
    std::env::var("AI_AUDIT_DB")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

fn now_epoch_secs() -> u64 {
    Utc::now().timestamp() as u64
}

/// After `git` exits; records cooldown and optional audit rows.
pub fn git_postcheck(args: &[String], exit_code: i32, gr: &EffectiveGitGuardrails) {
    let worker = std::env::var("AI_WORKER_ID").unwrap_or_else(|_| "unknown".into());
    let redact = gr.secrets_and_logs.redact_tokens_in_audit_and_app_logs;
    let argv_s = redact_argv_for_log(args, redact);

    if exit_code != 0 && gr.audit_policy.record_git_cli_failures {
        if let Some(db) = audit_db_path_opt() {
            if let Ok(log) = AuditLog::open(&db) {
                let _ = log.record_git_cli(&worker, &argv_s, false, Some("non-zero exit"));
            }
        }
    }

    if exit_code == 0 && git_argv_has_push(args) && gr.rate_limits.min_seconds_between_git_pushes > 0 {
        let path = push_cooldown_path();
        let _ = fs::write(path, format!("{}", now_epoch_secs()));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhKind {
    PrCreate,
    PrMerge,
    ApiMutation,
    Other,
}

pub fn classify_gh(args: &[String]) -> GhKind {
    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "--repo" || a == "-R" {
            i += 2;
            continue;
        }
        if a.starts_with('-') && a != "--" {
            i += 1;
            continue;
        }
        break;
    }
    if args.get(i).map(|s| s.as_str()) == Some("pr") {
        if args.get(i + 1).map(|s| s.as_str()) == Some("create") {
            return GhKind::PrCreate;
        }
        if args.get(i + 1).map(|s| s.as_str()) == Some("merge") {
            return GhKind::PrMerge;
        }
    }
    if args.get(i).map(|s| s.as_str()) == Some("api") {
        let rest = &args[i..];
        if gh_api_is_mutation(rest) {
            return GhKind::ApiMutation;
        }
    }
    GhKind::Other
}

fn gh_api_is_mutation(api_args: &[String]) -> bool {
    let mut method = "GET";
    let mut i = 1usize;
    while i < api_args.len() {
        match api_args[i].as_str() {
            "-X" | "--method" => {
                if let Some(m) = api_args.get(i + 1) {
                    method = m.as_str();
                }
                i += 2;
                continue;
            }
            _ => i += 1,
        }
    }
    matches!(
        method.to_uppercase().as_str(),
        "POST" | "PUT" | "PATCH" | "DELETE"
    )
}

pub fn gh_precheck(args: &[String], gr: &EffectiveGitGuardrails) -> std::result::Result<(), String> {
    repo_from_env()
        .as_deref()
        .map(|r| repo_allowed(gr, Some(r)))
        .unwrap_or_else(|| repo_allowed(gr, None))?;

    match classify_gh(args) {
        GhKind::PrMerge => {
            if !gr.merge_policy.allow_merging_pull_requests {
                return Err("guardrail: merging pull requests is disabled (mergePolicy)".into());
            }
        }
        GhKind::PrCreate => {
            let db_path = guard_db_path();
            let ctr = RollingCounter::open_pr_day(&db_path).map_err(|e| e.to_string())?;
            let n = ctr
                .count_in_window(
                    &std::env::var("AI_WORKER_ID").unwrap_or_else(|_| "unknown".into()),
                    chrono::Duration::hours(24),
                    Utc::now(),
                )
                .map_err(|e| e.to_string())?;
            if n >= gr.rate_limits.max_pull_requests_created_per_day {
                return Err(format!(
                    "guardrail: maxPullRequestsCreatedPerDay ({}) reached",
                    gr.rate_limits.max_pull_requests_created_per_day
                ));
            }
        }
        GhKind::ApiMutation => {
            let db_path = guard_db_path();
            let ctr =
                RollingCounter::open_github_mutations_hour(&db_path).map_err(|e| e.to_string())?;
            let n = ctr
                .count_in_window(
                    &std::env::var("AI_WORKER_ID").unwrap_or_else(|_| "unknown".into()),
                    chrono::Duration::hours(1),
                    Utc::now(),
                )
                .map_err(|e| e.to_string())?;
            if n >= gr.rate_limits.max_github_api_mutations_per_hour {
                return Err(format!(
                    "guardrail: maxGithubApiMutationsPerHour ({}) reached",
                    gr.rate_limits.max_github_api_mutations_per_hour
                ));
            }
        }
        GhKind::Other => {}
    }
    Ok(())
}

pub fn gh_postcheck(args: &[String], exit_code: i32, gr: &EffectiveGitGuardrails) {
    let worker = std::env::var("AI_WORKER_ID").unwrap_or_else(|_| "unknown".into());
    let redact = gr.secrets_and_logs.redact_tokens_in_audit_and_app_logs;
    let argv_s = redact_argv_for_log(args, redact);

    if exit_code != 0 && gr.audit_policy.record_git_cli_failures {
        if let Some(db) = audit_db_path_opt() {
            if let Ok(log) = AuditLog::open(&db) {
                let _ = log.record_git_cli(&worker, &argv_s, false, Some("gh non-zero exit"));
            }
        }
        return;
    }

    if exit_code != 0 {
        return;
    }

    let db_path = guard_db_path();
    match classify_gh(args) {
        GhKind::PrCreate => {
            if let Ok(ctr) = RollingCounter::open_pr_day(&db_path) {
                let _ = ctr.record_event(&worker, "pr_create", Utc::now());
            }
            if gr.audit_policy.record_all_github_mutations {
                if let Some(db) = audit_db_path_opt() {
                    if let Ok(log) = AuditLog::open(&db) {
                        let _ = log.record_github_mutation(
                            &worker,
                            "pr.create",
                            repo_from_env().as_deref(),
                            Some("POST"),
                            true,
                            None,
                        );
                    }
                }
            }
        }
        GhKind::PrMerge => {
            if gr.audit_policy.record_all_github_mutations {
                if let Some(db) = audit_db_path_opt() {
                    if let Ok(log) = AuditLog::open(&db) {
                        let _ = log.record_github_mutation(
                            &worker,
                            "pr.merge",
                            repo_from_env().as_deref(),
                            Some("POST"),
                            true,
                            None,
                        );
                    }
                }
            }
        }
        GhKind::ApiMutation => {
            if let Ok(ctr) = RollingCounter::open_github_mutations_hour(&db_path) {
                let _ = ctr.record_event(&worker, "github_mut", Utc::now());
            }
            if gr.audit_policy.record_all_github_mutations {
                if let Some(db) = audit_db_path_opt() {
                    if let Ok(log) = AuditLog::open(&db) {
                        let _ = log.record_github_mutation(
                            &worker,
                            "gh.api",
                            repo_from_env().as_deref(),
                            Some("MUT"),
                            true,
                            None,
                        );
                    }
                }
            }
        }
        GhKind::Other => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_delete_remote_when_configured() {
        let gr = EffectiveGitGuardrails {
            branch_safety: BranchSafety {
                forbid_delete_remote_branch: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let args = vec![
            "push".into(),
            "origin".into(),
            "--delete".into(),
            "foo".into(),
        ];
        assert!(git_precheck(&args, &gr).is_err());
    }

    #[test]
    fn allows_status() {
        let gr = EffectiveGitGuardrails::default();
        let args = vec!["status".into()];
        git_precheck(&args, &gr).unwrap();
    }

    #[test]
    fn classify_pr_create() {
        let args = vec!["pr".into(), "create".into(), "--title".into(), "x".into()];
        assert_eq!(classify_gh(&args), GhKind::PrCreate);
    }

    #[test]
    fn denylist_repo() {
        let gr = EffectiveGitGuardrails {
            scope: Scope {
                denied_repositories: vec!["evil/bad".into()],
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(repo_allowed(&gr, Some("evil/bad")).is_err());
    }
}
