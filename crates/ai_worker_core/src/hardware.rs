//! System RAM / CPU hints and suggested Ollama model (default: no GPU, 16GB → gemma4:e2b).

use serde::{Deserialize, Serialize};
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

pub fn probe_system() -> HardwareProfile {
    let mut sys = System::new_all();
    sys.refresh_memory();
    sys.refresh_cpu_all();

    let total_memory_bytes = sys.total_memory();
    let cpu_count = sys.cpus().len();

    // v1: conservative — no NVML wiring; default from plan (16GB-class CPU).
    // Larger RAM may justify a bigger tag later (e.g. gemma4 26B) once validated.
    let suggested_model = "gemma4:e2b";

    HardwareProfile {
        total_memory_bytes,
        cpu_count,
        suggested_model: suggested_model.to_string(),
        has_discrete_gpu_hint: false,
        notes: "GPU detection not implemented in v1; Ollama in Docker may still use GPU if available."
            .into(),
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
}
