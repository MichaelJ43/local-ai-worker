//! Hybrid Ollama bounded attempts + Cursor TypeScript SDK bridge (host `node`).
//!
//! Intended for orchestration inside the desktop app (`ai_worker_manager`) — not bundled in Docker agents.

pub mod bridge;
pub mod handoff;
pub mod pipeline;
pub mod verifier;

pub use bridge::{bridge_validate_only, build_bridge_payload, invoke_cursor_sdk_bridge};
pub use handoff::HandoffBullets;
pub use pipeline::bounded_ollama_attempts;
