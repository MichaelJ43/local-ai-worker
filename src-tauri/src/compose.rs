//! Materialize bundled Compose files to app data and run `docker compose`.

use std::path::{Path, PathBuf};
use std::process::Command;

const COMPOSE_BASE: &str = include_str!("../resources/compose/ollama-compose.base.yml");
const COMPOSE_GPU: &str = include_str!("../resources/compose/ollama-compose.gpu.yml");

const PROJECT_NAME: &str = "local-ai-ollama";

pub fn ollama_compose_dir(app_data_root: &Path) -> PathBuf {
    app_data_root.join("compose")
}

pub fn materialize_compose_files(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("ollama-compose.base.yml"), COMPOSE_BASE).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("ollama-compose.gpu.yml"), COMPOSE_GPU).map_err(|e| e.to_string())?;
    Ok(())
}

/// True if `nvidia-smi` lists GPUs (shared heuristic with [`ai_worker_core::hardware`]).
pub fn nvidia_smi_available() -> bool {
    ai_worker_core::hardware::nvidia_smi_available()
}

fn docker_compose_up_cmd(dir: &Path, use_gpu: bool) -> Result<Command, String> {
    materialize_compose_files(dir)?;
    let base = dir.join("ollama-compose.base.yml");
    let gpu = dir.join("ollama-compose.gpu.yml");
    if !base.exists() {
        return Err("missing ollama-compose.base.yml after materialize".into());
    }
    let mut cmd = Command::new("docker");
    cmd.current_dir(dir);
    cmd.args(["compose", "-p", PROJECT_NAME]);
    cmd.arg("-f").arg(&base);
    if use_gpu {
        cmd.arg("-f").arg(&gpu);
    }
    Ok(cmd)
}

fn output_to_string(out: std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    format!("{stdout}{stderr}")
}

pub fn stack_up(dir: &Path, use_gpu: bool) -> Result<String, String> {
    let mut cmd = docker_compose_up_cmd(dir, use_gpu)?;
    cmd.args(["up", "-d", "--remove-orphans"]);
    let out = cmd
        .output()
        .map_err(|e| format!("failed to run docker compose: {e}"))?;
    if !out.status.success() {
        return Err(output_to_string(out));
    }
    Ok(output_to_string(out))
}

/// Stops and removes containers for the compose project (no `-f` required once the project exists).
pub fn stack_down(dir: &Path) -> Result<String, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut cmd = Command::new("docker");
    cmd.current_dir(dir);
    cmd.args([
        "compose",
        "-p",
        PROJECT_NAME,
        "down",
        "--remove-orphans",
    ]);
    let out = cmd
        .output()
        .map_err(|e| format!("failed to run docker compose: {e}"))?;
    if !out.status.success() {
        return Err(output_to_string(out));
    }
    Ok(output_to_string(out))
}

pub fn stack_ps(dir: &Path) -> Result<String, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut cmd = Command::new("docker");
    cmd.current_dir(dir);
    cmd.args(["compose", "-p", PROJECT_NAME, "ps", "-a"]);
    let out = cmd
        .output()
        .map_err(|e| format!("failed to run docker compose: {e}"))?;
    if !out.status.success() {
        return Err(output_to_string(out));
    }
    Ok(output_to_string(out))
}

pub fn resolve_use_gpu(requested: Option<bool>) -> bool {
    match requested {
        Some(b) => b,
        None => nvidia_smi_available(),
    }
}
