//! Minimal Ollama HTTP client (list models, chat).

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::Result;

#[derive(Clone)]
pub struct OllamaClient {
    pub base_url: String,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl OllamaClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub async fn list_tags(&self) -> Result<Vec<String>> {
        let url = format!("{}/api/tags", self.base_url);
        let v: serde_json::Value = self.client.get(&url).send().await?.json().await?;
        let mut out = vec![];
        if let Some(models) = v.get("models").and_then(|m| m.as_array()) {
            for m in models {
                if let Some(name) = m.get("name").and_then(|n| n.as_str()) {
                    out.push(name.to_string());
                }
            }
        }
        Ok(out)
    }

    pub async fn chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        system: Option<&str>,
    ) -> Result<String> {
        let url = format!("{}/api/chat", self.base_url);
        let mut body = json!({
            "model": model,
            "messages": messages,
            "stream": false,
        });
        if let Some(s) = system {
            body.as_object_mut()
                .unwrap()
                .insert("system".into(), json!(s));
        }
        let resp: serde_json::Value = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;
        let content = resp
            .pointer("/message/content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        Ok(content)
    }
}
