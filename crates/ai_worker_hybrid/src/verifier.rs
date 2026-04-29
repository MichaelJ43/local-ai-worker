//! Pluggable verification after an Ollama response (strategy per task kind in future).

use std::path::Path;

/// Lightweight context passed to verifier plugins.
pub struct VerificationContext<'a> {
    pub workspace_root: &'a Path,
    #[allow(dead_code)]
    pub attempt_index: u32,
    #[allow(dead_code)]
    pub assistant_reply: &'a str,
}

pub struct VerificationOutcome {
    pub pass: bool,
    pub logs: String,
}

/// Task-type verifier hook (initial default: scripted pass/fail for tests and MVP wiring).
pub trait Verifier: Send + Sync {
    fn verify(&self, cx: &VerificationContext<'_>) -> VerificationOutcome;
}

/// Never passes verification (forces escalation after local bounded attempts exhaust).
#[derive(Debug, Clone, Copy, Default)]
pub struct AlwaysFailVerifier;

impl Verifier for AlwaysFailVerifier {
    fn verify(&self, _: &VerificationContext<'_>) -> VerificationOutcome {
        VerificationOutcome {
            pass: false,
            logs: "verifier stub: AlwaysFailVerifier".to_string(),
        }
    }
}

/// Always passes (use sparingly — short-circuits to “local succeeded” once reached).
#[derive(Debug, Clone, Copy, Default)]
pub struct AlwaysPassVerifier;

impl Verifier for AlwaysPassVerifier {
    fn verify(&self, _: &VerificationContext<'_>) -> VerificationOutcome {
        VerificationOutcome {
            pass: true,
            logs: "verifier stub: AlwaysPassVerifier".to_string(),
        }
    }
}
