use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("rules: {0}")]
    Rules(String),
    #[error("worker config: {0}")]
    WorkerConfig(String),
    #[error("ollama: {0}")]
    Ollama(String),
    #[error("docker: {0}")]
    Docker(String),
}
