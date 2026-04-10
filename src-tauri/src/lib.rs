//! Tauri backend: workers persistence, secrets, Docker/Ollama helpers.

mod compose;
mod worker_docker;

use ai_worker_core::{
    audit::AuditLog,
    docker, hardware,
    ollama::OllamaClient,
    rules::{self, RulesTree},
    worker_config::WorkerDefinition,
};
use keyring::Entry;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;

const KEYRING_SERVICE: &str = "local-ai-worker";
const GITHUB_TOKEN_USER: &str = "github_api_token";

fn app_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("local-ai-worker")
}

fn workers_path() -> PathBuf {
    app_dir().join("workers.json")
}

fn audit_db_path() -> PathBuf {
    app_dir().join("audit.sqlite3")
}

fn read_workers() -> Result<Vec<WorkerDefinition>, String> {
    let p = workers_path();
    if !p.exists() {
        return Ok(vec![]);
    }
    let data = std::fs::read_to_string(p).map_err(|e| e.to_string())?;
    serde_json::from_str(&data).map_err(|e| e.to_string())
}

fn write_workers(workers: &[WorkerDefinition]) -> Result<(), String> {
    for w in workers {
        w.validate()?;
    }
    let dir = app_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let data = serde_json::to_string_pretty(workers).map_err(|e| e.to_string())?;
    std::fs::write(workers_path(), data).map_err(|e| e.to_string())
}

fn rules_tree_cached() -> Result<RulesTree, String> {
    serde_json::from_str(ai_worker_core::DEFAULT_RULES_TREE_JSON).map_err(|e| e.to_string())
}

#[derive(Default)]
pub struct AppLogBuffer(Mutex<Vec<String>>);

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
    read_workers()
}

#[tauri::command]
fn save_workers(workers: Vec<WorkerDefinition>) -> Result<(), String> {
    write_workers(&workers)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
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
    Entry::new(KEYRING_SERVICE, GITHUB_TOKEN_USER)
        .and_then(|e| e.get_password())
        .map(|s| !s.is_empty())
        .unwrap_or(false)
}

#[tauri::command]
fn set_github_token(token: String) -> Result<(), String> {
    let entry = Entry::new(KEYRING_SERVICE, GITHUB_TOKEN_USER).map_err(|e| e.to_string())?;
    entry.set_password(&token).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_github_token() -> Result<(), String> {
    let entry = Entry::new(KEYRING_SERVICE, GITHUB_TOKEN_USER).map_err(|e| e.to_string())?;
    entry.delete_credential().map_err(|e| e.to_string())
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
    let mut workers = read_workers()?;
    let w = workers
        .iter_mut()
        .find(|x| x.id == worker_id)
        .ok_or_else(|| format!("worker not found: {worker_id}"))?;
    worker_docker::prepare_worker_storage(&app_dir(), w)?;
    let info = WorkerStorageInfo {
        context_path: w.context_path.clone().unwrap_or_default(),
        long_term_volume: w.long_term_volume.clone().unwrap_or_default(),
        container_name: worker_docker::container_name_for_worker(&worker_id),
    };
    write_workers(&workers)?;
    push_app_log(&log, format!("prepared storage for worker {worker_id}"));
    Ok(info)
}

#[tauri::command]
fn worker_docker_start(
    worker_id: String,
    log: tauri::State<AppLogBuffer>,
) -> Result<String, String> {
    let workers = read_workers()?;
    let w = workers
        .iter()
        .find(|x| x.id == worker_id)
        .ok_or_else(|| format!("worker not found: {worker_id}"))?;
    let r = worker_docker::worker_start(&app_dir(), &audit_db_path(), w);
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
    let workers = read_workers()?;
    let w = workers
        .iter()
        .find(|x| x.id == worker_id)
        .ok_or_else(|| format!("worker not found: {worker_id}"))?;
    let r = worker_docker::worker_recreate(&app_dir(), &audit_db_path(), w);
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
            docker_status,
            hardware_profile,
            github_token_configured,
            set_github_token,
            delete_github_token,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
