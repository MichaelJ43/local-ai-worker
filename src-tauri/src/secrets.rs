//! Named secrets in the OS keychain + index file under app data (names only).

use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const KEYRING_SERVICE: &str = "local-ai-worker";
const LEGACY_GITHUB_USER: &str = "github_api_token";

fn secret_entry_name(key: &str) -> String {
    format!("secret:{key}")
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SecretIndex {
    keys: Vec<String>,
}

fn index_path(app_root: &Path) -> PathBuf {
    app_root.join("secret_keys.json")
}

fn read_index(app_root: &Path) -> Result<SecretIndex, String> {
    let p = index_path(app_root);
    if !p.exists() {
        return Ok(SecretIndex::default());
    }
    let data = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
    serde_json::from_str(&data).map_err(|e| e.to_string())
}

fn write_index(app_root: &Path, idx: &SecretIndex) -> Result<(), String> {
    std::fs::create_dir_all(app_root).map_err(|e| e.to_string())?;
    let mut keys: BTreeSet<String> = idx.keys.iter().cloned().collect();
    keys.remove("");
    let mut v: Vec<String> = keys.into_iter().collect();
    v.sort();
    let idx = SecretIndex { keys: v };
    let data = serde_json::to_string_pretty(&idx).map_err(|e| e.to_string())?;
    std::fs::write(index_path(app_root), data).map_err(|e| e.to_string())
}

fn legacy_github_password() -> Option<String> {
    Entry::new(KEYRING_SERVICE, LEGACY_GITHUB_USER)
        .ok()
        .and_then(|e| e.get_password().ok())
        .filter(|s| !s.is_empty())
}

/// If the legacy single GitHub entry exists but no `github_token` in the KV index, import it.
pub fn migrate_legacy_github_if_needed(app_root: &Path) -> Result<(), String> {
    let mut idx = read_index(app_root)?;
    if idx.keys.iter().any(|k| k == "github_token") {
        return Ok(());
    }
    if let Some(pw) = legacy_github_password() {
        let entry = Entry::new(KEYRING_SERVICE, &secret_entry_name("github_token"))
            .map_err(|e| e.to_string())?;
        entry.set_password(&pw).map_err(|e| e.to_string())?;
        idx.keys.push("github_token".into());
        write_index(app_root, &idx)?;
    }
    Ok(())
}

pub fn validate_secret_key(key: &str) -> Result<(), String> {
    if key.is_empty() || key.len() > 64 {
        return Err("secret key: use 1–64 characters".into());
    }
    if !key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err("secret key: letters, digits, underscore only".into());
    }
    if !key.chars().next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false) {
        return Err("secret key: must start with a letter".into());
    }
    Ok(())
}

pub fn list_secret_keys(app_root: &Path) -> Result<Vec<String>, String> {
    migrate_legacy_github_if_needed(app_root)?;
    let idx = read_index(app_root)?;
    Ok(idx.keys)
}

pub fn set_secret(app_root: &Path, key: String, value: String) -> Result<(), String> {
    validate_secret_key(&key)?;
    if value.is_empty() {
        return Err("secret value cannot be empty".into());
    }
    let mut idx = read_index(app_root)?;
    if !idx.keys.contains(&key) {
        idx.keys.push(key.clone());
    }
    let entry = Entry::new(KEYRING_SERVICE, &secret_entry_name(&key)).map_err(|e| e.to_string())?;
    entry.set_password(&value).map_err(|e| e.to_string())?;
    write_index(app_root, &idx)?;
    Ok(())
}

pub fn delete_secret(app_root: &Path, key: &str) -> Result<(), String> {
    validate_secret_key(key)?;
    let mut idx = read_index(app_root)?;
    idx.keys.retain(|k| k != key);
    write_index(app_root, &idx)?;
    let entry = Entry::new(KEYRING_SERVICE, &secret_entry_name(key)).map_err(|e| e.to_string())?;
    let _ = entry.delete_credential();
    Ok(())
}

pub fn resolve_secret_value(key: &str) -> Option<String> {
    Entry::new(KEYRING_SERVICE, &secret_entry_name(key))
        .ok()
        .and_then(|e| e.get_password().ok())
        .filter(|s| !s.is_empty())
}

/// Prefer KV `github_token`, then legacy single-entry credential.
pub fn github_token_for_container() -> Option<String> {
    resolve_secret_value("github_token").or_else(legacy_github_password)
}

pub fn delete_legacy_github_entry() {
    let _ = Entry::new(KEYRING_SERVICE, LEGACY_GITHUB_USER).and_then(|e| e.delete_credential());
}

pub fn any_github_like_configured(app_root: &Path) -> Result<bool, String> {
    migrate_legacy_github_if_needed(app_root)?;
    Ok(resolve_secret_value("github_token").is_some() || legacy_github_password().is_some())
}
