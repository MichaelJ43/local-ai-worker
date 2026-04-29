//! Host-side bounded Ollama attempts + Cursor `@cursor/sdk` bridge (Node).

use ai_worker_core::llm_source::LlmSourceDefinition;
use ai_worker_core::ollama::OllamaClient;
use ai_worker_core::worker_config::WorkerDefinition;
use ai_worker_hybrid::verifier::AlwaysFailVerifier;
use ai_worker_hybrid::{
    bridge_validate_only, build_bridge_payload, bounded_ollama_attempts,
    invoke_cursor_sdk_bridge, HandoffBullets,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::State;

use crate::{push_app_log, secrets, AppLogBuffer};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HybridRunInputPayload {
    pub worker_id: String,
    #[serde(default)]
    pub skip_local_attempts: bool,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HybridRunEnvelope {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_finished: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_succeeded: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor_escalated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor_preview_snippet: Option<String>,
}

fn hybrid_log(app: &AppLogBuffer, line: impl Into<String>) {
    push_app_log(app, format!("[hybrid] {}", line.into()));
}

fn bridge_cli_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("cursor-agent-bridge")
        .join("cli.mjs")
}

pub(crate) fn resolve_workspace_from_worker(w: &WorkerDefinition) -> Result<PathBuf, String> {
    if let Some(h) = &w.hybrid_options {
        if let Some(p) = &h.workspace_path {
            let trimmed = p.trim();
            if !trimmed.is_empty() {
                let pb = PathBuf::from(trimmed);
                if pb.is_dir() {
                    return Ok(pb);
                }
                return Err(format!(
                    "Hybrid workspace_path is set but not a directory: {}",
                    pb.display()
                ));
            }
        }
    }

    let ctx = w
        .context_path
        .as_ref()
        .ok_or_else(|| "No context_path — run Prepare storage first.".to_string())?;
    let pb = Path::new(ctx)
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| "Could not derive workspace from context_path.".to_string())?;
    if !pb.is_dir() {
        return Err(format!(
            "Derived workspace path not a directory: {}",
            pb.display()
        ));
    }
    Ok(pb)
}

fn read_context_small(w: &WorkerDefinition) -> String {
    let path = match &w.context_path {
        Some(p) => p,
        None => return "{}".into(),
    };
    match std::fs::read_to_string(path) {
        Ok(raw) => {
            if raw.len() > 14_000 {
                raw.chars().take(14_000).collect::<String>() + " …truncated"
            } else {
                raw
            }
        }
        Err(_) => "(context file unreadable)".into(),
    }
}

fn task_instructions(w: &WorkerDefinition) -> String {
    w.tasks
        .iter()
        .map(|t| format!("• {} (task {})", t.title, t.id))
        .collect::<Vec<_>>()
        .join("\n")
}

#[tauri::command]
pub async fn hybrid_bridge_status() -> serde_json::Value {
    let bridge = bridge_cli_path();
    let sdk_dir = bridge
        .parent()
        .map(|x| x.join("node_modules").join("@cursor").join("sdk"));
    serde_json::json!({
        "bridgeCli": bridge.display().to_string(),
        "bridgeExists": bridge.exists(),
        "sdkInstalled": sdk_dir.as_ref().map(|d| d.is_dir()).unwrap_or(false),
        "hints": [
            "After clone run: `(cd cursor-agent-bridge && npm ci)` before using hybrid escalation.",
            "Node 20+ must be installed; `node` on PATH.",
            "Cursor API key: Secrets tab → key name defaults to cursor_api_key (Cursor Dashboard → Integrations)."
        ]
    })
}

#[tauri::command]
pub async fn hybrid_run_worker(
    app_log: State<'_, AppLogBuffer>,
    payload: HybridRunInputPayload,
) -> Result<HybridRunEnvelope, String> {
    let workers = crate::read_workers_disk_internal()?;
    let w = workers
        .into_iter()
        .find(|x| x.id == payload.worker_id)
        .ok_or_else(|| format!("worker id not found: {}", payload.worker_id))?;

    let sources = crate::persist_llm::read_llm_sources_raw()?;

    let hybrid = w.hybrid_options.clone().unwrap_or_default();

    let bridge = bridge_cli_path();
    if !bridge.exists() {
        hybrid_log(
            &app_log,
            format!("missing bridge {}", bridge.display()),
        );
        return Err(format!(
            "cursor-agent-bridge/cli.mjs missing at {} — check repository checkout",
            bridge.display()
        ));
    }

    let mut first_cursor: Option<&LlmSourceDefinition> = None;
    for tid in &w.escalation_path {
        if let Some(s) = ai_worker_core::llm_source::source_by_id(&sources, tid) {
            if matches!(s, LlmSourceDefinition::Cursor { .. }) {
                first_cursor = Some(s);
                break;
            }
        }
    }

    let (secret_key, cursor_model_for_bridge) = match first_cursor {
        Some(LlmSourceDefinition::Cursor {
            secret_key_name,
            cursor_model_id,
            ..
        }) => (secret_key_name.clone(), cursor_model_id.clone()),
        _ => {
            return Err(
                "Configure a Cursor LLM source in escalation path (LLM sources page) for hybrid escalation."
                    .into(),
            );
        }
    };

    secrets::validate_secret_key(&secret_key)?;

    let api_key_value = crate::secrets::resolve_secret_value(&secret_key)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            format!(
                "Add Cursor API secret `{}` via the Secrets tab (Cursor Dashboard → Integrations).",
                secret_key,
            )
        })?;

    let workspace_abs = resolve_workspace_from_worker(&w)?;

    hybrid_log(&app_log, format!("run worker {} hybrid escalate", w.id));

    let resolved = crate::persist_llm::resolve_worker_ollama_for_ops(&w, &sources)?;
    let ollama_base = resolved.host_ollama_base_url.clone();
    let model = resolved.model_for_agent_config.clone();

    let has_ollama_tier = w.escalation_path.iter().any(|id| {
        ai_worker_core::llm_source::source_by_id(&sources, id)
            .is_some_and(|s| matches!(s, LlmSourceDefinition::Ollama { .. }))
    });

    let skip_local_attempts =
        payload.skip_local_attempts || !has_ollama_tier;

    let (transcript_lines, verification_logs_vec, local_succeeded) =
        if skip_local_attempts {
            hybrid_log(&app_log, "skipped local bounded Ollama phase (no Ollama tier or requested)");
            (
                vec!["(local Ollama phase skipped)".into()],
                vec!["local phase skipped".into()],
                None,
            )
        } else {
            hybrid_log(
                &app_log,
                format!("starting bounded Ollama (model={})", model.as_str()),
            );
            let client = OllamaClient::new(&ollama_base);
            let per_ms =
                Duration::from_millis(hybrid.local_phase_timeout_ms.unwrap_or(120_000));
            let attempts = hybrid.local_max_attempts.unwrap_or(2).max(1);
            let user_prompt_task = format!(
                "Maintain autonomous worker goals. Scheduled tasks:\n{}\n",
                task_instructions(&w)
            );
            let out = bounded_ollama_attempts(
                &client,
                model.as_str(),
                None,
                &user_prompt_task,
                workspace_abs.as_path(),
                &AlwaysFailVerifier,
                attempts,
                per_ms,
            )
            .await;
            if let Some(er) = &out.last_error {
                hybrid_log(&app_log, format!("last local outcome: {}", er));
            }
            hybrid_log(&app_log, format!("local transcript lines {}", out.transcript.len()));
            (
                out.transcript,
                out.verification_logs,
                Some(out.succeeded),
            )
        };

    let bullets = HandoffBullets {
        task_instruction: format!("Worker `{}` ({}) — tasks:\n{}", w.name, w.id, task_instructions(&w)),
        repo_url: hybrid.repo_url.clone(),
        starting_ref: hybrid.starting_ref.clone(),
        context_json_excerpt: read_context_small(&w),
        ollama_transcript: transcript_lines,
        verification_logs: verification_logs_vec,
        workspace_root_display: Some(workspace_abs.display().to_string()),
    };
    let prompt_text = bullets.assemble_prompt_text();

    let repo_url_trim = hybrid
        .repo_url
        .as_ref()
        .map(|x| x.trim())
        .filter(|x| !x.is_empty())
        .unwrap_or("")
        .to_string();

    let mode_cloud = hybrid.allow_cloud_escalation && !repo_url_trim.is_empty();

    let bridged = build_bridge_payload(
        mode_cloud,
        Some(cursor_model_for_bridge.clone()),
        prompt_text.clone(),
        &workspace_abs,
        if mode_cloud {
            Some(repo_url_trim.as_str())
        } else {
            None
        },
        hybrid.starting_ref.as_deref(),
    );

    bridge_validate_only(&bridge, &bridged).map_err(|e| {
        format!(
            "bridge stdin validation failed ({e}). Run `(cd cursor-agent-bridge && npm ci)`."
        )
    })?;

    let json_out = invoke_cursor_sdk_bridge(&bridge, bridged, &api_key_value).map_err(|e| {
        hybrid_log(&app_log, format!("invoke failed {e}"));
        e
    })?;

    let snippet = json_out["assistantTextPreview"]
        .as_str()
        .unwrap_or("")
        .to_string();

    hybrid_log(
        &app_log,
        format!("cursor bridge ok {} chars preview", snippet.len()),
    );

    Ok(HybridRunEnvelope {
        ok: true,
        local_finished: Some(true),
        local_succeeded,
        cursor_escalated: Some(true),
        cursor_preview_snippet: Some(snippet.chars().take(4000).collect()),
    })
}
