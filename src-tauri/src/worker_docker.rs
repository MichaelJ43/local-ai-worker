//! Run/stop/recreate per-worker agent containers via Docker CLI.

use ai_worker_core::audit::AuditLog;
use ai_worker_core::context::WorkerContext;
use ai_worker_core::llm_source::ResolvedWorkerOllama;
use ai_worker_core::rules::{self, RulesTree};
use ai_worker_core::worker_config::WorkerDefinition;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::secrets;

pub fn container_name_for_worker(worker_id: &str) -> String {
    let safe: String = worker_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("local-ai-wkr-{safe}")
}

pub fn default_context_file(app_root: &Path, worker_id: &str) -> PathBuf {
    app_root.join("workers").join(worker_id).join("context.json")
}

pub fn default_long_term_volume_name(worker_id: &str) -> String {
    let safe = container_name_for_worker(worker_id)
        .trim_start_matches("local-ai-wkr-")
        .to_string();
    format!("local-ai-lt-{safe}")
}

fn default_agent_image(worker: &WorkerDefinition) -> String {
    worker
        .docker_image
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "local-ai-worker-agent:latest".to_string())
}

fn docker_volume_create(name: &str) -> Result<(), String> {
    let inspect = Command::new("docker")
        .args(["volume", "inspect", name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| e.to_string())?;
    if inspect.success() {
        return Ok(());
    }
    let st = Command::new("docker")
        .args(["volume", "create", name])
        .status()
        .map_err(|e| e.to_string())?;
    if !st.success() {
        return Err(format!("docker volume create {name} failed"));
    }
    Ok(())
}

/// Ensure dirs, context file, Docker volume; update worker paths in `w` and persist via caller.
pub fn prepare_worker_storage(
    app_root: &Path,
    w: &mut WorkerDefinition,
    ollama: &ResolvedWorkerOllama,
) -> Result<(), String> {
    let ctx = default_context_file(app_root, &w.id);
    std::fs::create_dir_all(ctx.parent().ok_or("context path")?).map_err(|e| e.to_string())?;
    let context = WorkerContext::load_or_create(&ctx).map_err(|e| e.to_string())?;
    context.save(&ctx).map_err(|e| e.to_string())?;
    w.context_path = Some(ctx.canonicalize().map_err(|e| e.to_string())?.to_string_lossy().into_owned());

    let vol = w
        .long_term_volume
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_long_term_volume_name(&w.id));
    docker_volume_create(&vol)?;
    w.long_term_volume = Some(vol);
    materialize_worker_runtime(app_root, w, ollama)?;
    Ok(())
}

/// Writes `guardrails.effective.json`, `system-prompt.txt`, and `agent-config.json` under `workers/<id>/`.
pub fn materialize_worker_runtime(
    app_root: &Path,
    w: &WorkerDefinition,
    ollama: &ResolvedWorkerOllama,
) -> Result<(), String> {
    let ctx_path = w
        .context_path
        .as_deref()
        .ok_or("worker has no contextPath — run prepare first")?;
    let wid_dir = app_root.join("workers").join(&w.id);
    std::fs::create_dir_all(&wid_dir).map_err(|e| e.to_string())?;

    let tree: RulesTree =
        serde_json::from_str(ai_worker_core::DEFAULT_RULES_TREE_JSON).map_err(|e| e.to_string())?;
    let (dom, rails) = rules::resolve_domain(
        &tree,
        &w.maintenance_domain,
        w.guardrail_overrides.as_ref(),
    )
    .map_err(|e| e.to_string())?;

    let guard_path = wid_dir.join("guardrails.effective.json");
    std::fs::write(
        &guard_path,
        serde_json::to_string_pretty(&rails).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let ctx = WorkerContext::load_or_create(ctx_path).map_err(|e| e.to_string())?;
    let excerpt = ctx.excerpt_for_prompt(4000);
    let mut full_prompt =
        rules::assemble_system_prompt_section(&dom, &rails, Some(&excerpt));

    let repo_trim = w
        .hybrid_options
        .as_ref()
        .and_then(|h| h.repo_url.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(repo) = repo_trim {
        full_prompt.push_str("\n\n### Repository (GitHub)\n");
        full_prompt.push_str(repo);
        full_prompt.push('\n');
    }

    let extra = w
        .worker_prompt
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(p) = extra {
        full_prompt.push_str("\n\n### Worker instructions\n");
        full_prompt.push_str(p);
        full_prompt.push('\n');
    }

    let prompt_path = wid_dir.join("system-prompt.txt");
    std::fs::write(&prompt_path, full_prompt).map_err(|e| e.to_string())?;

    let agent_cfg = serde_json::json!({
        "workerId": w.id,
        "model": ollama.model_for_agent_config,
        "ollamaHost": ollama.docker_env_host,
        "systemPromptPath": "/workspace/system-prompt.txt",
        "pollSeconds": 45,
        "tasks": w.tasks,
    });
    std::fs::write(
        wid_dir.join("agent-config.json"),
        serde_json::to_vec_pretty(&agent_cfg).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn collect_container_secret_env(w: &WorkerDefinition) -> Result<Vec<(String, String)>, String> {
    let mut seen = HashSet::<String>::new();
    let mut out = Vec::<(String, String)>::new();
    for b in &w.env_from_secrets {
        let ev = b.env_var.trim();
        let sk = b.secret_key.trim();
        if ev.is_empty() {
            return Err("envFromSecrets: envVar cannot be empty".into());
        }
        if sk.is_empty() {
            return Err(format!("envFromSecrets: secretKey required for {ev}"));
        }
        let val = if sk == "github_token" {
            secrets::github_token_for_container()
        } else {
            secrets::resolve_secret_value(sk)
        };
        let Some(val) = val else {
            return Err(format!(
                "secret '{sk}' is not set — add it under Secrets (or use github_token)"
            ));
        };
        if !seen.insert(ev.to_string()) {
            return Err(format!("duplicate env var in envFromSecrets: {ev}"));
        }
        out.push((ev.to_string(), val));
    }
    const GH: &str = "GITHUB_TOKEN";
    if !seen.contains(GH) {
        if let Some(tok) = secrets::github_token_for_container() {
            out.push((GH.to_string(), tok));
        }
    }
    Ok(out)
}

pub fn worker_start(
    app_root: &Path,
    audit_db: &Path,
    w: &WorkerDefinition,
    ollama: &ResolvedWorkerOllama,
) -> Result<String, String> {
    let ctx_path = w
        .context_path
        .as_deref()
        .ok_or("worker has no contextPath — run prepare first")?;
    let vol = w
        .long_term_volume
        .as_deref()
        .ok_or("worker has no longTermVolume — run prepare first")?;

    materialize_worker_runtime(app_root, w, ollama)?;
    AuditLog::open(audit_db).map_err(|e| e.to_string())?;

    let wid_dir = app_root.join("workers").join(&w.id);
    let guard_host = wid_dir.join("guardrails.effective.json");
    let prompt_host = wid_dir.join("system-prompt.txt");
    let agent_cfg_host = wid_dir.join("agent-config.json");
    let audit_host = audit_db
        .canonicalize()
        .map_err(|e| format!("audit db path: {e}"))?;

    let cname = container_name_for_worker(&w.id);
    let image = default_agent_image(w);

    let _ = Command::new("docker")
        .args(["rm", "-f", &cname])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    let ollama_host = ollama.docker_env_host.clone();
    let mut cmd = Command::new("docker");
    cmd.args(["run", "-d", "--name", &cname, "--restart", "unless-stopped"]);
    cmd.args(["--add-host", "host.docker.internal:host-gateway"]);
    cmd.arg("-v").arg(format!("{}:/workspace/context.json:rw", ctx_path));
    cmd.arg("-v").arg(format!("{}:/workspace/guardrails.effective.json:ro", guard_host.display()));
    cmd.arg("-v").arg(format!("{}:/workspace/system-prompt.txt:ro", prompt_host.display()));
    cmd.arg("-v").arg(format!("{}:/workspace/agent-config.json:ro", agent_cfg_host.display()));
    cmd.arg("-v").arg(format!("{}:/persist/audit.sqlite3:rw", audit_host.display()));
    cmd.arg("-v").arg(format!("{}:/persist", vol));
    cmd.args(["-w", "/workspace"]);
    cmd.args(["-e", &format!("OLLAMA_HOST={ollama_host}")]);
    cmd.args(["-e", &format!("AI_WORKER_ID={}", w.id)]);
    cmd.arg("-e").arg("AI_GUARD_STATE_DIR=/persist");
    cmd.arg("-e").arg("AI_AUDIT_DB=/persist/audit.sqlite3");
    cmd.arg("-e").arg("AI_CONTEXT_PATH=/workspace/context.json");
    cmd.arg("-e").arg("AI_AGENT_CONFIG=/workspace/agent-config.json");
    cmd.arg("-e").arg("AI_AGENT_LOOP=1");
    let secret_env = collect_container_secret_env(w)?;
    for (k, v) in secret_env {
        cmd.arg("-e").arg(format!("{k}={v}"));
    }
    cmd.arg(&image);

    let out = cmd.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn worker_logs(worker_id: &str, tail: usize) -> Result<String, String> {
    let cname = container_name_for_worker(worker_id);
    let out = Command::new("docker")
        .args([
            "logs",
            "--tail",
            &tail.to_string(),
            &cname,
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() && out.stdout.is_empty() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn worker_stop(worker_id: &str) -> Result<String, String> {
    let cname = container_name_for_worker(worker_id);
    let st = Command::new("docker")
        .args(["stop", "--time", "45", &cname])
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| e.to_string())?;
    if st.success() || st.code() == Some(1) {
        // Exit 1: no such container — treat as OK for idempotent disable.
    }
    Ok(format!("stop {cname} ({})", if st.success() { "ok" } else { "no container or already stopped" }))
}

pub fn worker_recreate(
    app_root: &Path,
    audit_db: &Path,
    w: &WorkerDefinition,
    ollama: &ResolvedWorkerOllama,
) -> Result<String, String> {
    let _ = worker_stop(&w.id)?;
    worker_start(app_root, audit_db, w, ollama)
}

/// Fully remove container, long-term volume (if set), and workspace directory under app data.
pub fn worker_teardown_all(app_root: &Path, w: &WorkerDefinition) -> Result<(), String> {
    let cname = container_name_for_worker(&w.id);
    let _ = Command::new("docker")
        .args(["rm", "-f", &cname])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    if let Some(vol) = w.long_term_volume.as_deref() {
        let v = vol.trim();
        if !v.is_empty() {
            let _ = Command::new("docker")
                .args(["volume", "rm", "-f", v])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
    let wid_dir = app_root.join("workers").join(&w.id);
    if wid_dir.is_dir() {
        std::fs::remove_dir_all(&wid_dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn worker_ps(worker_id: &str) -> Result<String, String> {
    let cname = container_name_for_worker(worker_id);
    let out = Command::new("docker")
        .args(["ps", "-a", "--filter", &format!("name=^{cname}$"), "--format", "{{.ID}} {{.Status}} {{.Names}}"])
        .output()
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
