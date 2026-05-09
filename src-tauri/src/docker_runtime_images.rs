//! Embedded catalog of runtime Docker images (see `resources/docker-runtime-images.json`) plus
//! per-worker refs. Local availability is queried via `docker image inspect` at runtime.

use std::collections::HashMap;
use std::process::Command;

use ai_worker_core::docker::docker_cli_available;
use ai_worker_core::worker_config::WorkerDefinition;
use serde::{Deserialize, Serialize};

use crate::worker_docker::{bundled_default_agent_image, resolved_agent_image};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogEntry {
    /// Stable row key in `docker-runtime-images.json` (tests and human editors).
    #[allow(dead_code)]
    id: String,
    pull_ref: String,
    display_name: String,
    category: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerRuntimeImageRow {
    pub pull_ref: String,
    pub display_name: String,
    pub category: String,
    pub present_locally: bool,
    pub image_id: Option<String>,
}

const EMBEDDED_CATALOG: &str = include_str!("../resources/docker-runtime-images.json");

fn load_catalog() -> Result<Vec<CatalogEntry>, String> {
    serde_json::from_str(EMBEDDED_CATALOG).map_err(|e| format!("docker-runtime-images.json: {e}"))
}

fn docker_local_image_id(image_ref: &str) -> Option<String> {
    let out = Command::new("docker")
        .args(["image", "inspect", "-f", "{{.Id}}", image_ref.trim()])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

#[derive(Default)]
struct Accum {
    labels: Vec<String>,
    categories: Vec<String>,
}

fn push_unique(vec: &mut Vec<String>, s: String) {
    if !vec.iter().any(|x| x == &s) {
        vec.push(s);
    }
}

fn primary_category(categories: &[String]) -> String {
    if categories.iter().any(|c| c == "ollama_stack") {
        "ollama_stack".into()
    } else {
        "worker_agent".into()
    }
}

/// Rows for Diagnostics: bundled catalog entries, compile-time default worker image, and each worker's resolved image.
pub fn docker_runtime_images_status(workers: &[WorkerDefinition]) -> Result<Vec<DockerRuntimeImageRow>, String> {
    let catalog = load_catalog()?;
    let docker_ok = docker_cli_available();

    let mut map: HashMap<String, Accum> = HashMap::new();

    for e in catalog {
        let pull_ref = e.pull_ref.trim().to_string();
        let acc = map.entry(pull_ref).or_default();
        push_unique(&mut acc.labels, e.display_name);
        push_unique(&mut acc.categories, e.category);
    }

    let bundled = bundled_default_agent_image().to_string();
    {
        let acc = map.entry(bundled).or_default();
        push_unique(&mut acc.labels, "Bundled default worker agent".into());
        push_unique(&mut acc.categories, "worker_agent".into());
    }

    for w in workers {
        let img = resolved_agent_image(w).trim().to_string();
        let acc = map.entry(img).or_default();
        let wlabel = if w.name.trim().is_empty() {
            w.id.clone()
        } else {
            w.name.trim().to_string()
        };
        push_unique(&mut acc.labels, format!("Worker «{wlabel}»"));
        push_unique(&mut acc.categories, "worker_agent".into());
    }

    let mut rows: Vec<DockerRuntimeImageRow> = map
        .into_iter()
        .map(|(pull_ref, acc)| {
            let category = primary_category(&acc.categories);
            let mut labels = acc.labels;
            labels.sort();
            let display_name = labels.join(" · ");
            let image_id = docker_ok.then(|| docker_local_image_id(&pull_ref)).flatten();
            let present_locally = image_id.is_some();
            DockerRuntimeImageRow {
                pull_ref,
                display_name,
                category,
                present_locally,
                image_id,
            }
        })
        .collect();

    rows.sort_by(|a, b| {
        let rank = |c: &str| -> u8 {
            match c {
                "ollama_stack" => 0,
                _ => 1,
            }
        };
        rank(&a.category)
            .cmp(&rank(&b.category))
            .then_with(|| a.pull_ref.cmp(&b.pull_ref))
    });

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_parses() {
        load_catalog().expect("embedded catalog");
    }

    #[test]
    fn ollama_catalog_matches_bundled_compose_yaml() {
        let yaml = include_str!("../resources/compose/ollama-compose.base.yml");
        let catalog = load_catalog().expect("catalog");
        let ollama = catalog
            .iter()
            .find(|e| e.id == "ollama_stack")
            .expect("ollama_stack entry");
        let needle = format!("image: {}", ollama.pull_ref.trim());
        assert!(
            yaml.contains(&needle),
            "compose YAML missing `{needle}` — sync ollama-compose.base.yml with docker-runtime-images.json"
        );
    }
}
