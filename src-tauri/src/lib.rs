//! Tauri backend: workers persistence, secrets, Docker/Ollama helpers.

mod compose;
mod hybrid;
mod persist_llm;
mod secrets;
mod worker_docker;
mod worker_repo_checkout;

use hybrid::{hybrid_bridge_status, hybrid_run_worker};

use ai_worker_core::{
    audit::AuditLog,
    docker, hardware,
    ollama::OllamaClient,
    rules::{self, RulesTree},
    worker_config::WorkerDefinition,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::Emitter;

pub(crate) fn app_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("local-ai-worker")
}

pub(crate) fn workers_path() -> PathBuf {
    app_dir().join("workers.json")
}

pub(crate) fn audit_db_path() -> PathBuf {
    app_dir().join("audit.sqlite3")
}

fn pending_restore_path() -> PathBuf {
    app_dir().join("pending_restore_prompt.json")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingRestoreFile {
    should_prompt: bool,
    enabled_by_worker_id: HashMap<String, bool>,
}

fn write_pending_restore_on_app_exit() {
    let _ = (|| -> Result<(), String> {
        let workers = read_workers_disk_internal()?;
        let should_prompt = workers.iter().any(|w| w.enabled);
        let enabled_by_worker_id: HashMap<_, _> =
            workers.iter().map(|w| (w.id.clone(), w.enabled)).collect();
        let doc = PendingRestoreFile {
            should_prompt,
            enabled_by_worker_id,
        };
        std::fs::create_dir_all(app_dir()).map_err(|e| e.to_string())?;
        std::fs::write(
            pending_restore_path(),
            serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })();
}

use ai_worker_core::llm_source::LlmSourceDefinition;

pub(crate) fn read_workers_disk_internal() -> Result<Vec<WorkerDefinition>, String> {
    let p = workers_path();
    if !p.exists() {
        return Ok(vec![]);
    }
    let data = std::fs::read_to_string(p).map_err(|e| e.to_string())?;
    serde_json::from_str(&data).map_err(|e| e.to_string())
}

pub(crate) fn write_workers_disk(workers: &[WorkerDefinition]) -> Result<(), String> {
    for w in workers {
        w.validate()?;
    }
    let dir = app_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let data = serde_json::to_string_pretty(workers).map_err(|e| e.to_string())?;
    std::fs::write(workers_path(), data).map_err(|e| e.to_string())
}

fn write_workers_with_llm(ws: &[WorkerDefinition], catalog: &[LlmSourceDefinition]) -> Result<(), String> {
    for w in ws {
        w.validate_with_llm_catalog(catalog)?;
    }
    write_workers_disk(ws)
}

fn rules_tree_cached() -> Result<RulesTree, String> {
    serde_json::from_str(ai_worker_core::DEFAULT_RULES_TREE_JSON).map_err(|e| e.to_string())
}

#[derive(Clone)]
pub struct AppLogBuffer(Arc<Mutex<Vec<String>>>);

impl Default for AppLogBuffer {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(Vec::new())))
    }
}

const APP_LOG_MAX: usize = 200;

fn push_app_log(buf: &AppLogBuffer, line: impl Into<String>) {
    let s = line.into();
    if let Ok(mut g) = buf.0.lock() {
        let ts = chrono::Local::now().format("%H:%M:%S").to_string();
        g.push(format!("{ts} {s}"));
        while g.len() > APP_LOG_MAX {
            g.remove(0);
        }
    }
}

#[tauri::command]
fn get_workers() -> Result<Vec<WorkerDefinition>, String> {
    persist_llm::ensure_migration()?;
    read_workers_disk_internal()
}

#[tauri::command]
fn get_llm_sources() -> Result<Vec<LlmSourceDefinition>, String> {
    persist_llm::read_llm_sources()
}

#[tauri::command]
fn save_llm_sources(sources: Vec<LlmSourceDefinition>) -> Result<(), String> {
    persist_llm::ensure_migration()?;
    persist_llm::write_llm_sources(&sources)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveWorkersResponse {
    runtime_pending: bool,
}

#[tauri::command]
fn save_workers(
    workers: Vec<WorkerDefinition>,
    app: tauri::AppHandle,
    log: tauri::State<AppLogBuffer>,
) -> Result<SaveWorkersResponse, String> {
    persist_llm::ensure_migration()?;
    let catalog = persist_llm::read_llm_sources_raw()?;
    let prev = read_workers_disk_internal().unwrap_or_default();
    write_workers_with_llm(&workers, &catalog)?;

    let prev_clone = prev;
    let log_clone = (*log).clone();
    let app_clone = app.clone();
    let _ = app.emit(
        "runtime-phase",
        serde_json::json!({ "message": "Applying worker runtime…" }),
    );
    std::thread::spawn(move || {
        push_app_log(&log_clone, "runtime apply (background)…");
        let outcome = persist_llm::apply_runtime_after_workers_save(&log_clone, &prev_clone);
        match outcome {
            Ok(()) => {
                push_app_log(&log_clone, "runtime apply finished");
                let _ = app_clone.emit(
                    "runtime-finished",
                    serde_json::json!({ "ok": true }),
                );
            }
            Err(e) => {
                push_app_log(&log_clone, format!("runtime apply ERR {e}"));
                let _ = app_clone.emit(
                    "runtime-error",
                    serde_json::json!({ "error": e }),
                );
            }
        }
    });

    Ok(SaveWorkersResponse {
        runtime_pending: true,
    })
}

#[tauri::command]
fn delete_worker(worker_id: String) -> Result<(), String> {
    persist_llm::ensure_migration()?;
    let catalog = persist_llm::read_llm_sources_raw()?;
    let mut all = read_workers_disk_internal()?;
    let idx = all
        .iter()
        .position(|x| x.id == worker_id)
        .ok_or_else(|| format!("worker not found: {worker_id}"))?;
    if all[idx].enabled {
        return Err("disable the worker before deleting it".into());
    }
    let victim = all.remove(idx);
    write_workers_with_llm(&all, &catalog)?;
    worker_docker::worker_teardown_all(&app_dir(), &victim)
}

#[tauri::command]
fn default_worker_agent_image() -> String {
    worker_docker::bundled_default_agent_image().to_string()
}

#[tauri::command]
fn worker_registry_images_refresh(log: tauri::State<AppLogBuffer>) -> Result<Vec<String>, String> {
    persist_llm::ensure_migration()?;
    let workers = read_workers_disk_internal()?;
    let mut refs = HashSet::<String>::new();
    refs.insert(worker_docker::bundled_default_agent_image().to_string());
    for w in &workers {
        refs.insert(worker_docker::resolved_agent_image(w));
    }
    let mut lines = Vec::new();
    for img in refs {
        match worker_docker::pull_worker_image_best_effort(&img) {
            Ok(msg) => {
                let line = format!("{img}: {msg}");
                push_app_log(&log, format!("worker image pull: {line}"));
                lines.push(line);
            }
            Err(e) => {
                let line = format!("{img}: ERR {e}");
                push_app_log(&log, format!("worker image pull: {line}"));
                lines.push(line);
            }
        }
    }
    Ok(lines)
}

#[derive(Serialize)]
struct DockerStatus {
    available: bool,
    version: Option<String>,
}

#[tauri::command]
fn docker_status() -> DockerStatus {
    if !docker::docker_cli_available() {
        return DockerStatus {
            available: false,
            version: None,
        };
    }
    DockerStatus {
        available: true,
        version: docker::docker_version_summary().ok(),
    }
}

#[tauri::command]
fn hardware_profile() -> ai_worker_core::hardware::HardwareProfile {
    hardware::probe_system()
}

#[tauri::command]
fn github_token_configured() -> bool {
    secrets::any_github_like_configured(&app_dir()).unwrap_or(false)
}

#[tauri::command]
fn set_github_token(token: String) -> Result<(), String> {
    secrets::set_secret(&app_dir(), "github_token".into(), token)?;
    secrets::delete_legacy_github_entry();
    Ok(())
}

#[tauri::command]
fn delete_github_token() -> Result<(), String> {
    let _ = secrets::delete_secret(&app_dir(), "github_token");
    secrets::delete_legacy_github_entry();
    Ok(())
}

#[tauri::command]
fn secret_keys_list() -> Result<Vec<String>, String> {
    secrets::list_secret_keys(&app_dir())
}

#[tauri::command]
fn secret_set(key: String, value: String) -> Result<(), String> {
    secrets::set_secret(&app_dir(), key, value)
}

#[tauri::command]
fn secret_delete(key: String) -> Result<(), String> {
    secrets::delete_secret(&app_dir(), &key)
}

#[tauri::command]
fn assemble_prompt_preview(
    domain: String,
    guardrail_overrides: Option<serde_json::Value>,
    context_excerpt: Option<String>,
) -> Result<String, String> {
    let tree = rules_tree_cached()?;
    let (dom, rails) =
        rules::resolve_domain(&tree, &domain, guardrail_overrides.as_ref()).map_err(|e| e.to_string())?;
    Ok(rules::assemble_system_prompt_section(
        &dom,
        &rails,
        context_excerpt.as_deref(),
    ))
}

#[tauri::command]
async fn ollama_list_models(host: Option<String>) -> Result<Vec<String>, String> {
    let base = host.unwrap_or_else(|| "http://127.0.0.1:11434".to_string());
    let c = OllamaClient::new(base);
    c.list_tags().await.map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OllamaGpuHint {
    nvidia_smi_available: bool,
    auto_use_gpu: bool,
    compose_dir: String,
}

#[tauri::command]
fn ollama_stack_gpu_hint() -> OllamaGpuHint {
    let dir = compose::ollama_compose_dir(&app_dir());
    OllamaGpuHint {
        nvidia_smi_available: compose::nvidia_smi_available(),
        auto_use_gpu: compose::resolve_use_gpu(None),
        compose_dir: dir.to_string_lossy().into_owned(),
    }
}

#[tauri::command]
fn ollama_stack_up(
    use_gpu: Option<bool>,
    log: tauri::State<AppLogBuffer>,
) -> Result<String, String> {
    let dir = compose::ollama_compose_dir(&app_dir());
    let gpu = compose::resolve_use_gpu(use_gpu);
    let r = compose::stack_up(&dir, gpu);
    match &r {
        Ok(s) => push_app_log(&log, format!("compose up: {s}")),
        Err(e) => push_app_log(&log, format!("compose up ERR: {e}")),
    }
    r
}

#[tauri::command]
fn ollama_stack_down(log: tauri::State<AppLogBuffer>) -> Result<String, String> {
    let dir = compose::ollama_compose_dir(&app_dir());
    let r = compose::stack_down(&dir);
    match &r {
        Ok(s) => push_app_log(&log, format!("compose down: {s}")),
        Err(e) => push_app_log(&log, format!("compose down ERR: {e}")),
    }
    r
}

#[tauri::command]
fn ollama_stack_status() -> Result<String, String> {
    let dir = compose::ollama_compose_dir(&app_dir());
    compose::stack_ps(&dir)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkerStorageInfo {
    context_path: String,
    long_term_volume: String,
    container_name: String,
}

#[tauri::command]
fn worker_storage_prepare(
    worker_id: String,
    log: tauri::State<AppLogBuffer>,
) -> Result<WorkerStorageInfo, String> {
    persist_llm::ensure_migration()?;
    let catalog = persist_llm::read_llm_sources_raw()?;
    let mut workers = read_workers_disk_internal()?;
    let (idx, ollama) = {
        let wi = workers
            .iter()
            .position(|x| x.id == worker_id)
            .ok_or_else(|| format!("worker not found: {worker_id}"))?;
        let o =
            persist_llm::resolve_worker_ollama_for_ops(&workers[wi], &catalog)?;
        (wi, o)
    };
    worker_docker::prepare_worker_storage(&app_dir(), &mut workers[idx], &ollama)?;
    let info = WorkerStorageInfo {
        context_path: workers[idx].context_path.clone().unwrap_or_default(),
        long_term_volume: workers[idx].long_term_volume.clone().unwrap_or_default(),
        container_name: worker_docker::container_name_for_worker(&worker_id),
    };
    write_workers_with_llm(&workers, &catalog)?;
    push_app_log(&log, format!("prepared storage for worker {worker_id}"));
    Ok(info)
}

#[tauri::command]
fn worker_docker_start(
    worker_id: String,
    log: tauri::State<AppLogBuffer>,
) -> Result<String, String> {
    persist_llm::ensure_migration()?;
    let catalog = persist_llm::read_llm_sources_raw()?;
    let workers = read_workers_disk_internal()?;
    let w = workers
        .iter()
        .find(|x| x.id == worker_id)
        .ok_or_else(|| format!("worker not found: {worker_id}"))?;
    let oll = persist_llm::resolve_worker_ollama_for_ops(w, &catalog)?;
    let r = worker_docker::worker_start(&app_dir(), &audit_db_path(), w, &oll);
    match &r {
        Ok(id) => push_app_log(&log, format!("worker {worker_id} start container {id}")),
        Err(e) => push_app_log(&log, format!("worker {worker_id} start ERR: {e}")),
    }
    r
}

#[tauri::command]
fn worker_docker_stop(
    worker_id: String,
    log: tauri::State<AppLogBuffer>,
) -> Result<String, String> {
    let r = worker_docker::worker_stop(&worker_id);
    match &r {
        Ok(s) => push_app_log(&log, format!("worker {worker_id} stop: {s}")),
        Err(e) => push_app_log(&log, format!("worker {worker_id} stop ERR: {e}")),
    }
    r
}

#[tauri::command]
fn worker_docker_recreate(
    worker_id: String,
    log: tauri::State<AppLogBuffer>,
) -> Result<String, String> {
    persist_llm::ensure_migration()?;
    let catalog = persist_llm::read_llm_sources_raw()?;
    let workers = read_workers_disk_internal()?;
    let w = workers
        .iter()
        .find(|x| x.id == worker_id)
        .ok_or_else(|| format!("worker not found: {worker_id}"))?;
    let oll = persist_llm::resolve_worker_ollama_for_ops(w, &catalog)?;
    let r = worker_docker::worker_recreate(&app_dir(), &audit_db_path(), w, &oll);
    match &r {
        Ok(s) => push_app_log(&log, format!("worker {worker_id} recreate: {s}")),
        Err(e) => push_app_log(&log, format!("worker {worker_id} recreate ERR: {e}")),
    }
    r
}

#[tauri::command]
fn worker_docker_status(worker_id: String) -> Result<String, String> {
    worker_docker::worker_ps(&worker_id)
}

#[tauri::command]
fn worker_docker_logs(worker_id: String, tail: Option<u64>) -> Result<String, String> {
    worker_docker::worker_logs(&worker_id, tail.unwrap_or(80) as usize)
}

#[tauri::command]
fn audit_record_github(
    worker_id: String,
    operation: String,
    repo: Option<String>,
    http_method: Option<String>,
    success: bool,
    detail: Option<String>,
) -> Result<(), String> {
    let db = AuditLog::open(audit_db_path()).map_err(|e| e.to_string())?;
    db.record_github_mutation(
        &worker_id,
        &operation,
        repo.as_deref(),
        http_method.as_deref(),
        success,
        detail.as_deref(),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn audit_recent_github(limit: u64) -> Result<Vec<serde_json::Value>, String> {
    let db = AuditLog::open(audit_db_path()).map_err(|e| e.to_string())?;
    db.recent_github_json(limit.min(500) as usize)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn app_log_lines(log: tauri::State<AppLogBuffer>) -> Vec<String> {
    log.0.lock().map(|g| g.clone()).unwrap_or_default()
}

/// Open a URL in the system browser (e.g. GitHub releases).
#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    open::that(url).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RulesDomainInfo {
    key: String,
    label: String,
    /// False when the rules-tree marks the domain as disabled (stub).
    selectable: bool,
}

#[tauri::command]
fn rules_domains_list() -> Result<Vec<RulesDomainInfo>, String> {
    let tree: serde_json::Value =
        serde_json::from_str(ai_worker_core::DEFAULT_RULES_TREE_JSON).map_err(|e| e.to_string())?;
    let domains = tree
        .get("domains")
        .and_then(|d| d.as_object())
        .ok_or_else(|| "rules tree missing domains".to_string())?;
    let mut out: Vec<RulesDomainInfo> = Vec::new();
    for (key, v) in domains {
        let label = v
            .get("label")
            .and_then(|x| x.as_str())
            .unwrap_or(key)
            .to_string();
        let enabled_flag = v
            .get("enabled")
            .and_then(|x| x.as_bool())
            .unwrap_or(true);
        out.push(RulesDomainInfo {
            key: key.clone(),
            label,
            selectable: enabled_flag,
        });
    }
    out.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(out)
}

#[tauri::command]
fn session_peek_pending_restore() -> Result<Option<PendingRestoreFile>, String> {
    let p = pending_restore_path();
    if !p.exists() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
    let doc: PendingRestoreFile = serde_json::from_str(&data).map_err(|e| e.to_string())?;
    if !doc.should_prompt {
        return Ok(None);
    }
    Ok(Some(doc))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionRestoreResponse {
    runtime_pending: bool,
}

#[tauri::command]
fn session_resolve_restore(
    choice: String,
    app: tauri::AppHandle,
    log: tauri::State<AppLogBuffer>,
) -> Result<SessionRestoreResponse, String> {
    persist_llm::ensure_migration()?;
    if choice == "dismiss" {
        let cleared = PendingRestoreFile {
            should_prompt: false,
            enabled_by_worker_id: HashMap::new(),
        };
        std::fs::create_dir_all(app_dir()).map_err(|e| e.to_string())?;
        std::fs::write(
            pending_restore_path(),
            serde_json::to_string_pretty(&cleared).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        return Ok(SessionRestoreResponse {
            runtime_pending: false,
        });
    }

    let prev = read_workers_disk_internal().unwrap_or_default();
    let mut workers = read_workers_disk_internal()?;
    let p = pending_restore_path();
    let snapshot: HashMap<String, bool> = if p.exists() {
        let data = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
        let doc: PendingRestoreFile = serde_json::from_str(&data).map_err(|e| e.to_string())?;
        doc.enabled_by_worker_id
    } else {
        HashMap::new()
    };

    match choice.as_str() {
        "restoreSnapshot" => {
            for w in &mut workers {
                if let Some(en) = snapshot.get(&w.id) {
                    w.enabled = *en;
                }
            }
        }
        "disableAll" => {
            for w in &mut workers {
                w.enabled = false;
            }
        }
        _ => return Err(format!("unknown session choice: {choice}")),
    }

    let catalog = persist_llm::read_llm_sources_raw()?;
    write_workers_with_llm(&workers, &catalog)?;

    let cleared = PendingRestoreFile {
        should_prompt: false,
        enabled_by_worker_id: HashMap::new(),
    };
    std::fs::create_dir_all(app_dir()).map_err(|e| e.to_string())?;
    std::fs::write(
        pending_restore_path(),
        serde_json::to_string_pretty(&cleared).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let prev_clone = prev;
    let log_clone = (*log).clone();
    let app_clone = app.clone();
    let _ = app.emit(
        "runtime-phase",
        serde_json::json!({ "message": "Applying session restore…" }),
    );
    std::thread::spawn(move || {
        push_app_log(&log_clone, "session restore runtime apply (background)…");
        let outcome = persist_llm::apply_runtime_after_workers_save(&log_clone, &prev_clone);
        match outcome {
            Ok(()) => {
                push_app_log(&log_clone, "session restore runtime finished");
                let _ = app_clone.emit(
                    "runtime-finished",
                    serde_json::json!({ "ok": true }),
                );
            }
            Err(e) => {
                push_app_log(&log_clone, format!("session restore runtime ERR {e}"));
                let _ = app_clone.emit(
                    "runtime-error",
                    serde_json::json!({ "error": e }),
                );
            }
        }
    });

    Ok(SessionRestoreResponse {
        runtime_pending: true,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppLogBuffer::default())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            get_workers,
            save_workers,
            get_llm_sources,
            save_llm_sources,
            delete_worker,
            docker_status,
            default_worker_agent_image,
            worker_registry_images_refresh,
            hardware_profile,
            github_token_configured,
            set_github_token,
            delete_github_token,
            secret_keys_list,
            secret_set,
            secret_delete,
            assemble_prompt_preview,
            ollama_list_models,
            ollama_stack_gpu_hint,
            ollama_stack_up,
            ollama_stack_down,
            ollama_stack_status,
            worker_storage_prepare,
            worker_docker_start,
            worker_docker_stop,
            worker_docker_recreate,
            worker_docker_status,
            worker_docker_logs,
            audit_record_github,
            audit_recent_github,
            app_log_lines,
            open_external_url,
            rules_domains_list,
            session_peek_pending_restore,
            session_resolve_restore,
            hybrid_bridge_status,
            hybrid_run_worker,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                write_pending_restore_on_app_exit();
            }
        });
}
