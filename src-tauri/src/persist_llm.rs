//! `llm_sources.json`, legacy migration, and Docker actions after toggling worker `enabled`.

use ai_worker_core::hardware;
use ai_worker_core::llm_source::{
    self, validate_llm_sources_catalog, LlmSourceDefinition, ResolvedWorkerOllama,
};
use ai_worker_core::worker_config::{HybridOptions, WorkerDefinition};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::{compose, app_dir, push_app_log, worker_docker, AppLogBuffer};

pub(crate) fn llm_sources_path() -> PathBuf {
    app_dir().join("llm_sources.json")
}

pub(crate) fn read_llm_sources_raw() -> Result<Vec<LlmSourceDefinition>, String> {
    let p = llm_sources_path();
    if !p.exists() {
        return Ok(vec![]);
    }
    let data = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
    serde_json::from_str(&data).map_err(|e| e.to_string())
}

pub(crate) fn read_llm_sources() -> Result<Vec<LlmSourceDefinition>, String> {
    ensure_migration()?;
    read_llm_sources_raw()
}

pub(crate) fn write_llm_sources(sources: &[LlmSourceDefinition]) -> Result<(), String> {
    validate_llm_sources_catalog(sources)?;
    std::fs::create_dir_all(app_dir()).map_err(|e| e.to_string())?;
    std::fs::write(
        llm_sources_path(),
        serde_json::to_string_pretty(sources).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn hybrid_legacy_cursor_hint(ho: &HybridOptions) -> bool {
    ho.cursor_model_id
        .as_ref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
        || ho
            .cursor_secret_key
            .as_ref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
}

/// One-shot migration into `llm_sources.json`, fills `escalationPath`, clears mirrored fields from workers.
pub(crate) fn ensure_migration() -> Result<(), String> {
    let mut workers = crate::read_workers_disk_internal()?;
    let mut sources = read_llm_sources_raw()?;

    if !sources.is_empty() {
        return Ok(());
    }
    let suggested = hardware::probe_system().suggested_model;
    let fallback_host = "http://127.0.0.1:11434".to_string();

    let legacy = workers.iter().any(|w| {
        w.model_override.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false)
            || w.ollama_host.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false)
            || w.hybrid_options
                .as_ref()
                .map(hybrid_legacy_cursor_hint)
                .unwrap_or(false)
    });

    if workers.is_empty() || !legacy {
        return Ok(());
    }

    let mut pair_to_olla: HashMap<(String, String), String> = HashMap::new();
    let mut pair_to_cur: HashMap<(String, String), String> = HashMap::new();

    for w in &workers {
        let host = w
            .ollama_host
            .clone()
            .unwrap_or_else(|| fallback_host.clone())
            .trim()
            .to_string();
        let host = if host.is_empty() {
            fallback_host.clone()
        } else {
            host
        };
        let model = w
            .model_override
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| suggested.clone());
        let key = (host.clone(), model.clone());
        pair_to_olla
            .entry(key)
            .or_insert_with(|| uuid::Uuid::new_v4().to_string());

        if let Some(ho) = &w.hybrid_options {
            let mid = ho.cursor_model_id.clone().unwrap_or_default();
            if mid.trim().is_empty()
                && ho
                    .cursor_secret_key
                    .as_ref()
                    .map(|s| s.trim())
                    .unwrap_or("")
                    .is_empty()
            {
                continue;
            }
            let mk = ho.cursor_model_id.clone().unwrap_or_else(|| "".into()).trim().to_string();
            if mk.is_empty() {
                continue;
            }
            let sk = ho
                .cursor_secret_key
                .clone()
                .unwrap_or_else(|| "cursor_api_key".into())
                .trim()
                .to_string();
            let sk = if sk.is_empty() {
                "cursor_api_key".into()
            } else {
                sk
            };
            pair_to_cur
                .entry((sk, mk.clone()))
                .or_insert_with(|| uuid::Uuid::new_v4().to_string());
        }
    }

    sources = Vec::new();
    let mut tuples: Vec<((String, String), String)> = pair_to_olla.into_iter().collect();
    tuples.sort_by(|a, b| a.1.cmp(&b.1));
    for ((base_url, default_model), id) in tuples {
        sources.push(LlmSourceDefinition::Ollama {
            id,
            name: "Migrated — local Ollama".into(),
            base_url,
            default_model,
        });
    }

    let mut curt: Vec<((String, String), String)> = pair_to_cur.into_iter().collect();
    curt.sort_by(|a, b| a.1.cmp(&b.1));
    for ((secret_key_name, cursor_model_id), id) in curt {
        sources.push(LlmSourceDefinition::Cursor {
            id,
            name: "Migrated — Cursor".into(),
            cursor_model_id,
            secret_key_name,
        });
    }

    let olla_map: HashMap<(String, String), String> = sources
        .iter()
        .filter_map(|s| match s {
            LlmSourceDefinition::Ollama {
                id,
                base_url,
                default_model,
                ..
            } => Some(((base_url.clone(), default_model.clone()), id.clone())),
            _ => None,
        })
        .collect();
    let cur_map: HashMap<(String, String), String> = sources
        .iter()
        .filter_map(|s| match s {
            LlmSourceDefinition::Cursor {
                id,
                cursor_model_id,
                secret_key_name,
                ..
            } => Some((
                (
                    secret_key_name.clone(),
                    cursor_model_id.clone(),
                ),
                id.clone(),
            )),
            _ => None,
        })
        .collect();

    for w in &mut workers {
        let host = w
            .ollama_host
            .clone()
            .unwrap_or_else(|| fallback_host.clone())
            .trim()
            .to_string();
        let host = if host.is_empty() {
            fallback_host.clone()
        } else {
            host
        };
        let model = w
            .model_override
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| suggested.clone());
        let oid = olla_map
            .get(&(host, model.clone()))
            .cloned()
            .or_else(|| olla_map.values().next().cloned())
            .unwrap();

        let mut path = vec![oid.clone()];
        if let Some(ref ho) = w.hybrid_options {
            let mid = ho.cursor_model_id.clone().unwrap_or_default().trim().to_string();
            if !mid.is_empty() {
                let sk = ho
                    .cursor_secret_key
                    .clone()
                    .unwrap_or_else(|| "cursor_api_key".into())
                    .trim()
                    .to_string();
                let sk = if sk.is_empty() {
                    "cursor_api_key".into()
                } else {
                    sk
                };
                if let Some(cid) = cur_map.get(&(sk, mid.clone())) {
                    path.push(cid.clone());
                }
            }
        }

        w.escalation_path = path;

        if let Some(mut ho) = w.hybrid_options.take() {
            ho.cursor_secret_key = None;
            ho.cursor_model_id = None;
            let nonempty = ho
                .repo_url
                .as_ref()
                .map(|x| !x.trim().is_empty())
                .unwrap_or(false)
                || ho.workspace_path.as_ref().map(|s| !s.trim().is_empty()) == Some(true)
                || ho.starting_ref.as_ref().map(|s| !s.trim().is_empty()) == Some(true)
                || ho.local_phase_timeout_ms.is_some()
                || ho.local_max_attempts.is_some();
            let allow_default = HybridOptions::default().allow_cloud_escalation;
            if nonempty || ho.allow_cloud_escalation != allow_default {
                w.hybrid_options = Some(ho);
            } else {
                w.hybrid_options = None;
            }
        }
        w.model_override = None;
        w.ollama_host = None;
    }

    write_llm_sources(&sources)?;

    crate::write_workers_disk(&workers)?;

    Ok(())
}

pub(crate) fn resolve_worker_ollama_for_ops(
    w: &WorkerDefinition,
    sources: &[LlmSourceDefinition],
) -> Result<ResolvedWorkerOllama, String> {
    let suggested = hardware::probe_system().suggested_model;
    if !w.escalation_path.is_empty() {
        return llm_source::resolve_first_ollama_for_escalation(sources, &w.escalation_path);
    }
    let base = w
        .ollama_host
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:11434".into());
    let model = w
        .model_override
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or(suggested);
    Ok(ResolvedWorkerOllama {
        host_ollama_base_url: base.clone(),
        docker_env_host: llm_source::docker_ollama_host_from_base(&base),
        model_for_agent_config: model,
    })
}

fn any_enabled_worker_needs_loopback_stack(
    workers: &[WorkerDefinition],
    sources: &[LlmSourceDefinition],
) -> bool {
    workers.iter().any(|w| {
        llm_source::enabled_worker_needs_loopback_ollama_stack(w, sources)
    })
}

pub(crate) fn apply_runtime_after_workers_save(
    log: &AppLogBuffer,
    prev_workers: &[WorkerDefinition],
) -> Result<(), String> {
    let sources = read_llm_sources_raw()?;
    let mut cur = crate::read_workers_disk_internal()?;

    let prev_map: HashMap<String, WorkerDefinition> =
        prev_workers.iter().cloned().map(|w| (w.id.clone(), w)).collect();

    let compose_dir = compose::ollama_compose_dir(&crate::app_dir());
    let use_gpu = compose::resolve_use_gpu(None);
    let mut compose_up_done = false;

    for idx in 0..cur.len() {
        let id = cur[idx].id.clone();
        let was_en = prev_map.get(&id).map(|p| p.enabled).unwrap_or(false);
        let now_en = cur[idx].enabled;

        if was_en && !now_en {
            match worker_docker::worker_stop(&id) {
                Ok(msg) => push_app_log(log, format!("worker {id} disable {msg}")),
                Err(e) => {
                    push_app_log(log, format!("worker {id} disable ERR {e}"));
                    return Err(e.to_string());
                }
            }
            continue;
        }

        if !was_en && now_en {
            if llm_source::enabled_worker_needs_loopback_ollama_stack(&cur[idx], &sources)
                && !compose_up_done
            {
                push_app_log(log, "ollama compose: stack up (local Ollama)…");
                match compose::stack_up(&compose_dir, use_gpu) {
                    Ok(out) => {
                        push_app_log(log, format!("compose up OK: {out}"));
                        compose_up_done = true;
                    }
                    Err(e) => {
                        push_app_log(log, format!("compose up ERR {e}"));
                        return Err(e);
                    }
                }
            }

            let mut ollama = resolve_worker_ollama_for_ops(&cur[idx], &sources)?;

            let need_prep = cur[idx].context_path.is_none()
                || cur[idx]
                    .long_term_volume
                    .as_ref()
                    .is_none_or(|v| v.trim().is_empty())
                || cur[idx]
                    .context_path
                    .as_ref()
                    .map(|p| !std::path::Path::new(p).exists())
                    .unwrap_or(true);

            if need_prep {
                worker_docker::prepare_worker_storage(&crate::app_dir(), &mut cur[idx], &ollama)?;
                crate::write_workers_disk(&cur)?;
            }

            ollama = resolve_worker_ollama_for_ops(&cur[idx], &sources)?;
            let r = worker_docker::worker_start(
                &crate::app_dir(),
                &crate::audit_db_path(),
                &cur[idx],
                &ollama,
            );
            match &r {
                Ok(s) => push_app_log(log, format!("worker {id} docker start OK {s}")),
                Err(e) => {
                    push_app_log(log, format!("worker {id} docker start ERR {e}"));
                    return Err(e.to_string());
                }
            };
        }
    }

    let snap = crate::read_workers_disk_internal()?;
    if !any_enabled_worker_needs_loopback_stack(&snap, &sources) {
        push_app_log(log, "ollama compose: stack down (no enabled loopback Ollama workers)…");
        match compose::stack_down(&compose_dir) {
            Ok(out) => push_app_log(log, format!("compose down OK: {out}")),
            Err(e) => {
                push_app_log(log, format!("compose down ERR {e}"));
                return Err(e);
            }
        }
    }

    Ok(())
}
