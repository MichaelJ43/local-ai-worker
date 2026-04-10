//! Load and merge `rules-tree.json`; assemble system prompt fragments for a domain.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use crate::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulesTree {
    pub version: u32,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    pub domains: BTreeMap<String, DomainRules>,
}

#[derive(Debug, Clone)]
pub struct DomainRules {
    pub label: String,
    pub enabled: Option<bool>,
    pub guardrails: serde_json::Value,
    pub prompt_guidelines: Vec<String>,
}

impl<'de> Deserialize<'de> for DomainRules {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper {
            #[serde(default)]
            label: String,
            #[serde(default)]
            enabled: Option<bool>,
            #[serde(default)]
            guardrails: serde_json::Value,
            #[serde(default, alias = "promptGuidelines")]
            prompt_guidelines: Vec<String>,
        }
        let h = Helper::deserialize(deserializer)?;
        Ok(DomainRules {
            label: h.label,
            enabled: h.enabled,
            guardrails: h.guardrails,
            prompt_guidelines: h.prompt_guidelines,
        })
    }
}

impl Serialize for DomainRules {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("DomainRules", 4)?;
        s.serialize_field("label", &self.label)?;
        s.serialize_field("enabled", &self.enabled)?;
        s.serialize_field("guardrails", &self.guardrails)?;
        s.serialize_field("promptGuidelines", &self.prompt_guidelines)?;
        s.end()
    }
}

pub fn load_rules_tree_from_path(path: impl AsRef<Path>) -> Result<RulesTree> {
    let bytes = std::fs::read(path)?;
    let tree: RulesTree = serde_json::from_slice(&bytes)?;
    Ok(tree)
}

/// Deep-merge `overrides` into base JSON (objects merge recursively; arrays and scalars replace).
pub fn merge_json(base: &mut serde_json::Value, overrides: &serde_json::Value) {
    match (base, overrides) {
        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
            for (k, v) in b {
                if let Some(existing) = a.get_mut(k) {
                    merge_json(existing, v);
                } else {
                    a.insert(k.clone(), v.clone());
                }
            }
        }
        (base_slot, new_val) => {
            *base_slot = new_val.clone();
        }
    }
}

pub fn resolve_domain(
    tree: &RulesTree,
    domain_key: &str,
    guardrail_overrides: Option<&serde_json::Value>,
) -> Result<(DomainRules, serde_json::Value)> {
    let domain = tree
        .domains
        .get(domain_key)
        .ok_or_else(|| crate::Error::Rules(format!("unknown domain: {domain_key}")))?;
    if domain.enabled == Some(false) {
        return Err(crate::Error::Rules(format!(
            "domain {domain_key} is disabled"
        )));
    }
    let mut guardrails = domain.guardrails.clone();
    if let Some(ov) = guardrail_overrides {
        merge_json(&mut guardrails, ov);
    }
    let mut resolved = domain.clone();
    resolved.guardrails = guardrails.clone();
    Ok((resolved, guardrails))
}

/// Build a single system-style prompt section from domain rules and optional worker context summary.
pub fn assemble_system_prompt_section(
    domain: &DomainRules,
    guardrails_json: &serde_json::Value,
    worker_context_excerpt: Option<&str>,
) -> String {
    let mut parts = vec![format!("## Domain: {}", domain.label)];
    parts.push("### Guardrails (JSON)\n```json".to_string());
    parts.push(
        serde_json::to_string_pretty(guardrails_json)
            .unwrap_or_else(|_| "{}".to_string()),
    );
    parts.push("```\n".to_string());
    if !domain.prompt_guidelines.is_empty() {
        parts.push("### Guidelines".to_string());
        for (i, g) in domain.prompt_guidelines.iter().enumerate() {
            parts.push(format!("{}. {}", i + 1, g));
        }
    }
    if let Some(ex) = worker_context_excerpt {
        if !ex.is_empty() {
            parts.push(format!("\n### Worker context (excerpt)\n{}", ex));
        }
    }
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_embedded_sample_rules() {
        let sample = include_str!("../../../docs/rules/rules-tree.json");
        let tree: RulesTree = serde_json::from_str(sample).expect("parse");
        assert_eq!(tree.version, 1);
        assert!(tree.domains.contains_key("git"));
    }

    #[test]
    fn merge_overrides_nested() {
        let mut base = serde_json::json!({
            "rateLimits": { "maxPullRequestsCreatedPerDay": 10 },
            "scope": { "enforceRepositoryAllowlist": false }
        });
        let ov = serde_json::json!({
            "rateLimits": { "maxPullRequestsCreatedPerDay": 3 }
        });
        merge_json(&mut base, &ov);
        assert_eq!(base["rateLimits"]["maxPullRequestsCreatedPerDay"], 3);
        assert_eq!(base["scope"]["enforceRepositoryAllowlist"], false);
    }
}
