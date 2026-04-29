//! Structured hand-off from local attempts to Cursor SDK bridge (stdin JSON schema).

use serde::{Deserialize, Serialize};

/// Human-readable bullets + machine context appended as the Cursor user prompt text.
#[derive(Debug, Clone, Default)]
pub struct HandoffBullets {
    pub task_instruction: String,
    pub repo_url: Option<String>,
    pub starting_ref: Option<String>,
    pub context_json_excerpt: String,
    pub ollama_transcript: Vec<String>,
    pub verification_logs: Vec<String>,
    pub workspace_root_display: Option<String>,
}

impl HandoffBullets {
    /// Build the full prompt handed to Cursor (markdown-ish, plain text).
    pub fn assemble_prompt_text(&self) -> String {
        let mut s = String::new();
        s.push_str("## Hybrid worker hand-off (from Local AI Worker)\n\n");
        s.push_str("### Task\n");
        s.push_str(&self.task_instruction);
        s.push_str("\n\n");
        if let Some(u) = &self.repo_url {
            s.push_str("### Repo\n");
            s.push_str("- ");
            s.push_str(u);
            if let Some(r) = &self.starting_ref {
                s.push_str(" @ ");
                s.push_str(r);
            }
            s.push('\n');
        }
        if let Some(w) = &self.workspace_root_display {
            s.push_str("### Workspace (host path)\n");
            s.push_str(w);
            s.push('\n');
        }
        s.push_str("\n### Saved context snapshot\n```json\n");
        s.push_str(&self.context_json_excerpt);
        s.push_str("\n```\n");

        if !self.ollama_turns_trimmed().is_empty() {
            s.push_str("\n### Local Ollama attempts (truncated transcript)\n");
            for line in self.ollama_turns_trimmed() {
                s.push_str("- ");
                s.push_str(&line);
                s.push('\n');
            }
        }
        if !self.verification_logs.is_empty() {
            s.push_str("\n### Verification / scripts\n");
            for line in &self.verification_logs {
                s.push_str("- ");
                s.push_str(line);
                s.push('\n');
            }
        }
        s.push_str("\nPlease continue using the toolchain appropriate for this repository.\n");
        s
    }

    fn ollama_turns_trimmed(&self) -> Vec<String> {
        self.ollama_transcript
            .iter()
            .map(|t| truncate_line(t, 4000))
            .collect()
    }
}

fn truncate_line(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{cut}…({count} chars total)")
}

#[cfg(test)]
mod handoff_tests {
    use super::*;

    #[test]
    fn assemble_contains_task_instruction() {
        let h = HandoffBullets {
            task_instruction: "Bump lodash".into(),
            repo_url: Some("https://github.com/org/repo.git".into()),
            starting_ref: Some("main".into()),
            context_json_excerpt: r#"{"version":1}"#.into(),
            ollama_transcript: vec!["try 1".into()],
            verification_logs: vec!["lint failed".into()],
            workspace_root_display: Some("/tmp/ws".into()),
        };
        let t = h.assemble_prompt_text();
        assert!(t.contains("Bump lodash"));
        assert!(t.contains("github.com"));
        assert!(t.contains("/tmp/ws"));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeStdin {
    #[serde(default)]
    pub version: u32,
    /// `"local"` uses `local.cwd`; `"cloud"` uses `cloud.repos`.
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    pub prompt_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local: Option<BridgeStdinLocal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud: Option<BridgeStdinCloud>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeStdinLocal {
    pub cwd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeStdinCloud {
    pub repos: Vec<BridgeRepoRef>,
    #[serde(default)]
    pub auto_create_pr: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeRepoRef {
    pub url: String,
    #[serde(default)]
    pub starting_ref: Option<String>,
}
