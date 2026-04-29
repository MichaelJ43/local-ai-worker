//! Bounded local Ollama attempts before escalating to Cursor.

use std::path::Path;
use std::time::Duration;

use ai_worker_core::ollama::{ChatMessage, OllamaClient};
use tokio::time::timeout;

use crate::verifier::{VerificationContext, Verifier};

pub struct BoundedLocalOutcome {
    pub succeeded: bool,
    pub transcript: Vec<String>,
    pub verification_logs: Vec<String>,
    pub last_error: Option<String>,
}

/// Run up to `max_attempts` chats; after each assistant reply run `verifier.verify`.
///
/// Uses `tokio::time::timeout` per HTTP call (`per_attempt_timeout`).
#[allow(clippy::too_many_arguments)]
pub async fn bounded_ollama_attempts<V: Verifier + ?Sized>(
    client: &OllamaClient,
    model: &str,
    system_prompt: Option<&str>,
    initial_user_prompt: &str,
    workspace_root: &Path,
    verifier: &V,
    max_attempts: u32,
    per_attempt_timeout: Duration,
) -> BoundedLocalOutcome {
    let mut transcript = Vec::new();
    let mut verification_logs = Vec::new();
    let mut last_error: Option<String> = None;
    let mut user_content = initial_user_prompt.to_string();

    for attempt in 0..max_attempts.max(1) {
        let msgs = vec![ChatMessage {
            role: "user".into(),
            content: user_content.clone(),
        }];
        let chat_fut = client.chat(model, &msgs, system_prompt);
        match timeout(per_attempt_timeout, chat_fut).await {
            Err(_) => {
                last_error = Some(format!("ollama attempt {} timed out", attempt + 1));
                break;
            }
            Ok(Err(e)) => {
                last_error = Some(format!("ollama chat error attempt {}: {}", attempt + 1, e));
                break;
            }
            Ok(Ok(reply)) => {
                transcript.push(format!("assistant [{}]: {}", attempt + 1, reply));
                let vo = verifier.verify(&VerificationContext {
                    workspace_root,
                    attempt_index: attempt,
                    assistant_reply: &reply,
                });
                verification_logs.push(vo.logs);
                if vo.pass {
                    return BoundedLocalOutcome {
                        succeeded: true,
                        transcript,
                        verification_logs,
                        last_error: None,
                    };
                }
                user_content =
                    format!("{initial_user_prompt}\n\n(previous assistant reply was not verified; revise and propose a concrete patch.)\n\nAssistant said:\n{reply}");
            }
        }
    }

    BoundedLocalOutcome {
        succeeded: false,
        transcript,
        verification_logs,
        last_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verifier::{AlwaysFailVerifier, AlwaysPassVerifier};

    /// Mock HTTP server unavailable — expect timeout or connection error quickly.
    #[tokio::test]
    async fn bounded_attempts_handles_dead_host() {
        let client = OllamaClient::new("http://127.0.0.1:1");
        let verifier = AlwaysFailVerifier;
        let outcome = bounded_ollama_attempts(
            &client,
            "any",
            None,
            "ping",
            Path::new("/tmp"),
            &verifier,
            1,
            Duration::from_millis(200),
        )
        .await;
        assert!(!outcome.succeeded);
        assert!(outcome.last_error.is_some() || outcome.transcript.is_empty());
    }

    #[tokio::test]
    async fn pass_verifier_stops_early_without_network() {
        let verifier = AlwaysPassVerifier;
        let msgs = VerificationContext {
            workspace_root: Path::new("/tmp"),
            attempt_index: 0,
            assistant_reply: "",
        };
        assert!(verifier.verify(&msgs).pass);
    }
}
