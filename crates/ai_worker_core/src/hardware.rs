//! System RAM / CPU hints, GPU heuristics, and suggested Ollama model (default: gemma4:e2b).

use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};
use sysinfo::System;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareProfile {
    pub total_memory_bytes: u64,
    pub cpu_count: usize,
    pub suggested_model: String,
    pub has_discrete_gpu_hint: bool,
    pub notes: String,
}

/// True when `nvidia-smi -L` succeeds and reports at least one GPU.
/// Used by the Ollama compose GPU toggle and [`probe_system`].
pub fn nvidia_smi_available() -> bool {
    Command::new("nvidia-smi")
        .args(["-L"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .map(|o| {
            o.status.success()
                && !o.stdout.is_empty()
                && String::from_utf8_lossy(&o.stdout).to_lowercase().contains("gpu")
        })
        .unwrap_or(false)
}

fn nvidia_primary_vram_mib() -> Option<u64> {
    let out = Command::new("nvidia-smi")
        .args([
            "--query-gpu=memory.total",
            "--format=csv,noheader,nounits",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout);
    let first = line.lines().next()?.trim();
    let mib: u64 = first.parse().ok()?;
    Some(mib)
}

#[cfg(target_os = "macos")]
fn macos_gpu_note() -> Option<String> {
    if cfg!(target_arch = "aarch64") {
        return Some(
            "Apple Silicon — Metal-capable; local Ollama can use GPU acceleration.".into(),
        );
    }
    let out = Command::new("system_profiler")
        .args(["SPDisplaysDataType", "-json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let arr = v.get("SPDisplaysDataType")?.as_array()?;
    let mut names = Vec::new();
    for item in arr {
        if let Some(name) = item.get("sppci_model").and_then(|x| x.as_str()) {
            names.push(name.to_string());
        } else if let Some(name) = item.get("_name").and_then(|x| x.as_str()) {
            names.push(name.to_string());
        }
    }
    if names.is_empty() {
        return None;
    }
    Some(format!("Display adapter(s): {}", names.join(", ")))
}

#[cfg(not(target_os = "macos"))]
fn macos_gpu_note() -> Option<String> {
    None
}

#[cfg(target_os = "linux")]
fn linux_lspci_gpu_note() -> Option<String> {
    let out = Command::new("lspci").stdout(Stdio::piped()).stderr(Stdio::null()).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let lower = line.to_lowercase();
        if !(lower.contains("vga")
            || lower.contains("3d controller")
            || lower.contains("display controller"))
        {
            continue;
        }
        if lower.contains("nvidia") {
            return Some(format!("NVIDIA adapter (lspci): {}", line.trim()));
        }
        if lower.contains("advanced micro devices") || lower.contains("amd/ati") {
            return Some(format!("AMD adapter (lspci): {}", line.trim()));
        }
        if lower.contains("intel") && (lower.contains("arc") || lower.contains("iris")) {
            return Some(format!("Intel GPU (lspci): {}", line.trim()));
        }
    }
    None
}

#[cfg(not(target_os = "linux"))]
fn linux_lspci_gpu_note() -> Option<String> {
    None
}

#[cfg(target_os = "windows")]
fn meaningful_windows_gpu_name(name: &str) -> bool {
    let n = name.to_lowercase();
    if n.contains("microsoft basic render")
        || n.contains("microsoft remote display")
        || n == "microsoft basic display adapter"
    {
        return false;
    }
    n.contains("nvidia")
        || n.contains("amd")
        || n.contains("radeon")
        || n.contains("intel arc")
        || n.contains("rtx")
        || n.contains("gtx")
        || n.contains("rx ")
}

#[cfg(target_os = "windows")]
fn windows_video_adapter_note() -> Option<String> {
    const CMD: &str =
        "Get-CimInstance Win32_VideoController | Select-Object -ExpandProperty Name";
    let out = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            CMD,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let names: Vec<String> = text
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if names.is_empty() {
        return None;
    }
    let meaningful: Vec<String> = names
        .iter()
        .filter(|n| meaningful_windows_gpu_name(n))
        .cloned()
        .collect();
    if meaningful.is_empty() {
        return None;
    }
    Some(format!("Display adapter(s): {}", meaningful.join(", ")))
}

#[cfg(not(target_os = "windows"))]
fn windows_video_adapter_note() -> Option<String> {
    None
}

pub fn probe_system() -> HardwareProfile {
    let mut sys = System::new_all();
    sys.refresh_memory();
    sys.refresh_cpu_all();

    let total_memory_bytes = sys.total_memory();
    let cpu_count = sys.cpus().len();

    let mut hints: Vec<String> = Vec::new();
    let mut has_gpu_hint = false;

    if nvidia_smi_available() {
        has_gpu_hint = true;
        let mut s = "NVIDIA GPU detected (nvidia-smi).".to_string();
        if let Some(mib) = nvidia_primary_vram_mib() {
            s.push_str(&format!(" First adapter reports ~{mib} MiB VRAM."));
        }
        hints.push(s);
    } else if let Some(s) = macos_gpu_note() {
        has_gpu_hint = true;
        hints.push(s);
    } else if let Some(s) = linux_lspci_gpu_note() {
        has_gpu_hint = true;
        hints.push(s);
    } else if let Some(s) = windows_video_adapter_note() {
        has_gpu_hint = true;
        hints.push(s);
    }

    let suggested_model = "gemma4:e2b".to_string();

    let notes = if hints.is_empty() {
        "No GPU detected via nvidia-smi / platform heuristics; CPU or remote Ollama is fine. Docker Ollama can still use a GPU when the NVIDIA Container Toolkit is configured.".into()
    } else {
        format!("{} Suggested tag is conservative; increase model size if you have ample VRAM.", hints.join(" "))
    };

    HardwareProfile {
        total_memory_bytes,
        cpu_count,
        suggested_model,
        has_discrete_gpu_hint: has_gpu_hint,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_runs() {
        let p = probe_system();
        assert!(p.total_memory_bytes > 0);
        assert!(!p.suggested_model.is_empty());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn meaningful_windows_gpu_name_filters_basic() {
        assert!(!super::meaningful_windows_gpu_name("Microsoft Basic Render Driver"));
        assert!(super::meaningful_windows_gpu_name("NVIDIA GeForce RTX 3080"));
    }
}
