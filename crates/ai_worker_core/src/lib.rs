//! Core library: rules, context, audit, Ollama client, hardware hints, rate limits, scheduling.

/// Bundled default rules tree (workspace `docs/rules/rules-tree.json`).
pub const DEFAULT_RULES_TREE_JSON: &str = include_str!("../../../docs/rules/rules-tree.json");

pub mod audit;
pub mod context;
pub mod docker;
pub mod error;
pub mod guard_exec;
pub mod hardware;
pub mod llm_source;
pub mod ollama;
pub mod rate_limits;
pub mod rules;
pub mod scheduler;
pub mod worker_config;

pub use error::Error;

pub type Result<T> = std::result::Result<T, Error>;
