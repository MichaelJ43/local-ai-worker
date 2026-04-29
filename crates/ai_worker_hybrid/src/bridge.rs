//! Spawn `node cursor-agent-bridge/cli.mjs` with hand-off JSON on stdin.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::handoff::{BridgeRepoRef, BridgeStdin, BridgeStdinCloud, BridgeStdinLocal};

/// Sends JSON stdin to bridge; parses the last stdout line as JSON.
pub fn invoke_cursor_sdk_bridge<P: AsRef<Path>>(
    bridge_cli_js: P,
    payload: BridgeStdin,
    api_key: &str,
) -> Result<serde_json::Value, String> {
    let mut child = Command::new("node")
        .arg(
            bridge_cli_js
                .as_ref()
                .as_os_str()
                .to_str()
                .ok_or_else(|| {
                    "cursor bridge path contains invalid unicode (expected utf-8)".to_string()
                })?,
        )
        .env("CURSOR_API_KEY", api_key)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn node bridge: {e}"))?;

    let body = serde_json::to_vec(&payload).map_err(|e| e.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&body).map_err(|e| e.to_string())?;
    }

    let out = child
        .wait_with_output()
        .map_err(|e| format!("cursor bridge subprocess: {e}"))?;

    let stderr_txt = String::from_utf8_lossy(&out.stderr).trim().to_string();
    let stdout_txt = String::from_utf8_lossy(&out.stdout);

    let last_json_line = stdout_txt.lines().rev().find_map(|ln| {
        let t = ln.trim();
        if t.starts_with('{') {
            serde_json::from_str::<serde_json::Value>(t).ok()
        } else {
            None
        }
    });

    if !out.status.success() {
        return Err(if stderr_txt.is_empty() {
            format!(
                "cursor bridge exited with {} (stdout={}, lastJson={:?})",
                out.status.code().unwrap_or(-1),
                stdout_txt.trim(),
                last_json_line
            )
        } else {
            format!("cursor bridge failed: {stderr_txt}")
        });
    }

    last_json_line.ok_or_else(|| {
        format!(
            "cursor bridge stdout had no JSON line (stdout={:?})",
            stdout_txt.chars().take(500).collect::<String>()
        )
    })
}

/// Validates stdin JSON serialization only (runs `cli.mjs --validate-only`).
pub fn bridge_validate_only<P: AsRef<Path>>(
    bridge_cli_js: P,
    payload: &BridgeStdin,
) -> Result<(), String> {
    let json = serde_json::to_vec(payload).map_err(|e| e.to_string())?;

    let out = Command::new("node")
        .args([
            bridge_cli_js.as_ref().to_str().ok_or("bridge path utf-8")?,
            "--validate-only",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("CURSOR_API_KEY")
        .spawn()
        .and_then(|mut c| {
            if let Some(mut i) = c.stdin.take() {
                use std::io::Write as _;
                i.write_all(&json)?;
            }
            c.wait_with_output()
        })
        .map_err(|e| e.to_string())?;

    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "validate-only exit {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

/// Decide whether to use Cursor cloud repos vs local cwd based on hybrid policy.
pub fn build_bridge_payload(
    mode_cloud: bool,
    model_id: Option<String>,
    prompt_text: String,
    workspace_abs: impl AsRef<Path>,
    repo_url: Option<&str>,
    starting_ref: Option<&str>,
) -> BridgeStdin {
    let workspace_abs = workspace_abs.as_ref().to_string_lossy().to_string();

    if mode_cloud {
        let url = repo_url.unwrap_or_default().trim().to_string();
        BridgeStdin {
            version: 1,
            mode: "cloud".into(),
            model_id,
            prompt_text,
            local: None,
            cloud: Some(BridgeStdinCloud {
                repos: vec![BridgeRepoRef {
                    url,
                    starting_ref: Some(
                        starting_ref
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "main".into()),
                    ),
                }],
                auto_create_pr: false,
            }),
        }
    } else {
        BridgeStdin {
            version: 1,
            mode: "local".into(),
            model_id,
            prompt_text,
            local: Some(BridgeStdinLocal {
                cwd: workspace_abs,
            }),
            cloud: None,
        }
    }
}

/// Quick JSON shape sanity for UI/diagnostics (`ok` booleans inside node output).
#[cfg(test)]
mod tests_diag {
    #[test]
    fn sample_success_line_parses() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"ok":true,"mode":"local","runId":"r1","assistantTextPreview":"hi"}"#,
        )
        .unwrap();
        assert_eq!(v["ok"].as_bool(), Some(true));
    }
}
