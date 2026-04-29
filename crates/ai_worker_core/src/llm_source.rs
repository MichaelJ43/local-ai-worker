//! Saved LLM backend definitions (persisted beside `workers.json`).

use serde::{Deserialize, Serialize};

/// One saved source the UI and escalation path reference by stable `id`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LlmSourceDefinition {
    #[serde(rename = "ollama")]
    Ollama {
        id: String,
        name: String,
        #[serde(rename = "baseUrl")]
        base_url: String,
        #[serde(rename = "defaultModel")]
        default_model: String,
    },
    #[serde(rename = "cursor")]
    Cursor {
        id: String,
        name: String,
        #[serde(rename = "cursorModelId")]
        cursor_model_id: String,
        #[serde(rename = "secretKeyName")]
        secret_key_name: String,
    },
}

impl LlmSourceDefinition {
    pub fn id(&self) -> &str {
        match self {
            LlmSourceDefinition::Ollama { id, .. } | LlmSourceDefinition::Cursor { id, .. } => id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            LlmSourceDefinition::Ollama { name, .. } | LlmSourceDefinition::Cursor { name, .. } => {
                name
            }
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.id().trim().is_empty() {
            return Err("llm source id empty".into());
        }
        if self.name().trim().is_empty() {
            return Err("llm source name empty".into());
        }
        match self {
            LlmSourceDefinition::Ollama {
                base_url,
                default_model,
                ..
            } => {
                let u = base_url.trim();
                if u.is_empty() {
                    return Err(format!("baseUrl required for {}", self.id()));
                }
                if !(u.starts_with("http://") || u.starts_with("https://")) {
                    return Err(format!("baseUrl must start with http:// or https:// (source {})", self.id()));
                }
                if default_model.trim().is_empty() {
                    return Err(format!("defaultModel required for {}", self.id()));
                }
            }
            LlmSourceDefinition::Cursor {
                cursor_model_id,
                secret_key_name,
                ..
            } => {
                if cursor_model_id.trim().is_empty() {
                    return Err(format!("cursorModelId required for {}", self.id()));
                }
                if secret_key_name.trim().is_empty() {
                    return Err(format!("secretKeyName required for {}", self.id()));
                }
            }
        }
        Ok(())
    }
}

pub fn validate_llm_sources_catalog(sources: &[LlmSourceDefinition]) -> Result<(), String> {
    let mut ids = std::collections::HashSet::<String>::new();
    for s in sources {
        s.validate()?;
        if !ids.insert(s.id().to_string()) {
            return Err(format!("duplicate llm source id: {}", s.id()));
        }
    }
    Ok(())
}

pub fn source_by_id<'a>(
    sources: &'a [LlmSourceDefinition],
    id: &str,
) -> Option<&'a LlmSourceDefinition> {
    sources.iter().find(|s| s.id() == id)
}

/// Map an Ollama base URL toward an address reachable from inside an agent container.
///
/// Loopback-ish URLs are rewritten through `host.docker.internal` (preserving `:port`).
/// LAN / LAN-style URLs are passed through (with implicit `http://` prefix when missing).
pub fn docker_ollama_host_from_base(base_url: &str) -> String {
    let t = base_url.trim();
    if t.is_empty() {
        return "http://host.docker.internal:11434".into();
    }
    let low = t.to_ascii_lowercase();
    if low.contains("127.0.0.1") || low.contains("localhost") {
        let mut port = "11434";
        if let Some(idx) = t.rfind(':') {
            let after = &t[idx + 1..];
            let p = after
                .split(&['/', '?'][..])
                .next()
                .unwrap_or("");
            if !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) {
                port = p;
            }
        }
        return format!("http://host.docker.internal:{port}");
    }
    if low.starts_with("http://") || low.starts_with("https://") {
        t.into()
    } else {
        format!("http://{t}")
    }
}

/// Resolved bindings for Docker agent injection and host-side Ollama clients (hybrid / preview).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedWorkerOllama {
    /// Reachable HTTP base from the desktop host (`127.0.0.1` / LAN as saved in the catalog).
    pub host_ollama_base_url: String,
    /// Inject into `OLLAMA_HOST` inside the Linux agent container.
    pub docker_env_host: String,
    pub model_for_agent_config: String,
}
pub fn resolve_first_ollama_for_escalation(
    sources: &[LlmSourceDefinition],
    escalation: &[String],
) -> Result<ResolvedWorkerOllama, String> {
    for sid in escalation {
        match source_by_id(sources, sid) {
            Some(LlmSourceDefinition::Ollama {
                base_url,
                default_model,
                ..
            }) => {
                let base = base_url.trim().to_string();
                if base.is_empty() {
                    continue;
                }
                let inject = docker_ollama_host_from_base(&base);
                let model = default_model.trim().to_string();
                if model.is_empty() {
                    continue;
                }
                return Ok(ResolvedWorkerOllama {
                    host_ollama_base_url: base,
                    docker_env_host: inject,
                    model_for_agent_config: model,
                });
            }
            Some(LlmSourceDefinition::Cursor { .. }) => {}
            None => {}
        }
    }
    Err(
        "escalation path must include at least one Ollama source for Docker agent workloads"
            .into(),
    )
}
