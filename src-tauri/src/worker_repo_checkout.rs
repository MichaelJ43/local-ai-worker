//! Managed git checkout under workers/<id>/checkout for Docker bind-mount.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Host directory that will be mounted at `/workspace/repo` in the agent container.
pub fn checkout_directory(app_root: &Path, worker_id: &str) -> PathBuf {
    app_root.join("workers").join(worker_id).join("checkout")
}

/// Embed a GitHub PAT for `git clone` / `fetch` over HTTPS without persisting PAT in callers.
pub fn authenticated_github_https_url(repo_url: &str, token: Option<&str>) -> String {
    let u = repo_url.trim();
    let Some(tok) = token.map(str::trim).filter(|t| !t.is_empty()) else {
        return u.to_string();
    };
    if u.contains('@') {
        return u.to_string();
    }
    let rest = if let Some(r) = u.strip_prefix("https://github.com/") {
        r
    } else if let Some(r) = u.strip_prefix("https://www.github.com/") {
        r
    } else {
        return u.to_string();
    };
    format!("https://x-access-token:{tok}@github.com/{rest}")
}

fn run_git_in_dir(checkout: &Path, args: &[&str]) -> Result<(), String> {
    let mut c = Command::new("git");
    c.current_dir(checkout);
    for a in args {
        c.arg(a);
    }
    c.env("GIT_TERMINAL_PROMPT", "0");
    let st = c.status().map_err(|e| {
        format!(
            "could not spawn `git`: {e} — install Git CLI on the host to use repo checkout"
        )
    })?;
    if !st.success() {
        return Err(format!("git {} failed ({})", args.join(" "), st));
    }
    Ok(())
}

fn run_git_in_dir_capture(checkout: &Path, args: &[&str]) -> Result<String, String> {
    let mut c = Command::new("git");
    c.current_dir(checkout).args(args);
    c.env("GIT_TERMINAL_PROMPT", "0");
    let out = c.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let ok = String::from_utf8_lossy(&out.stdout);
        return Err(format!("git {} failed: {err}{ok}", args.join(" ")));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Keep `ensure_repo_checkout` idempotent across runs.
pub fn ensure_repo_checkout(
    repo_url: &str,
    checkout: &Path,
    preferred_ref: Option<&str>,
    github_pat: Option<&str>,
) -> Result<(), String> {
    let u = repo_url.trim();
    if u.is_empty() {
        return Ok(());
    }
    let Some(parent) = checkout.parent() else {
        return Err("checkout path has no parent directory".into());
    };
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;

    let auth_url = authenticated_github_https_url(u, github_pat);
    let git_marker = checkout.join(".git");

    if git_marker.exists() {
        sync_repo(&auth_url, checkout, preferred_ref, u)?;
    } else {
        if checkout.exists() {
            std::fs::remove_dir_all(checkout).map_err(|e| e.to_string())?;
        }
        clone_repo(&auth_url, checkout, preferred_ref)?;
        restore_remote_without_token(checkout, u)?;
    }

    Ok(())
}

fn clone_repo(auth_url: &str, checkout: &Path, branch: Option<&str>) -> Result<(), String> {
    let mut cmd = Command::new("git");
    cmd.arg("clone");
    cmd.args(["--depth", "120"]);
    if let Some(b) = branch.map(str::trim).filter(|x| !x.is_empty()) {
        cmd.arg("-b").arg(b);
    }
    cmd.arg(auth_url).arg(checkout);
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    let status = cmd.status().map_err(|e| format!("git clone spawn: {e}"))?;
    if !status.success() {
        return Err(
            "git clone failed — check HTTPS GitHub URL, network, and github_token secret"
                .into(),
        );
    }
    Ok(())
}

fn restore_remote_without_token(checkout: &Path, public_https_url: &str) -> Result<(), String> {
    run_git_in_dir(
        checkout,
        &["remote", "set-url", "origin", public_https_url.trim()],
    )
}

/// Fetch with auth URL, fast-forward, then restore public `origin` URL (no token on disk).
fn sync_repo(
    auth_url_for_fetch: &str,
    checkout: &Path,
    preferred_ref: Option<&str>,
    origin_public_https: &str,
) -> Result<(), String> {
    let _ = run_git_in_dir(checkout, &["remote", "set-url", "origin", auth_url_for_fetch]);
    let inner = (|| {
        run_git_in_dir(
            checkout,
            &["fetch", "--depth", "120", "--prune", "origin"],
        )?;
        checkout_tree(checkout, preferred_ref)?;
        let _ = run_git_in_dir_capture(checkout, &["pull", "--ff-only", "--no-edit"]);
        Ok::<(), String>(())
    })();
    let _ = restore_remote_without_token(checkout, origin_public_https.trim());
    inner
}

fn checkout_tree(checkout: &Path, preferred_ref: Option<&str>) -> Result<(), String> {
    if let Some(r) = preferred_ref.map(str::trim).filter(|x| !x.is_empty()) {
        if run_git_in_dir(checkout, &["checkout", r]).is_ok() {
            return Ok(());
        }
        let or = format!("origin/{r}");
        if run_git_in_dir(checkout, &["checkout", "-B", r, &or]).is_ok() {
            return Ok(());
        }
    }

    if let Some(def) = detect_default_branch_hint(checkout) {
        let tb = format!("origin/{def}");
        if run_git_in_dir(checkout, &["checkout", "-B", &def, &tb]).is_ok()
            || run_git_in_dir(checkout, &["checkout", &def]).is_ok()
        {
            return Ok(());
        }
    }

    if run_git_in_dir(checkout, &["checkout", "main"]).is_ok()
        || run_git_in_dir(checkout, &["checkout", "master"]).is_ok()
    {
        Ok(())
    } else {
        Err("checkout: could not select main/master or remote default branch after fetch".into())
    }
}

fn detect_default_branch_hint(checkout: &Path) -> Option<String> {
    let sym = run_git_in_dir_capture(
        checkout,
        &["symbolic-ref", "refs/remotes/origin/HEAD", "--short"],
    )
    .ok()?;
    sym.strip_prefix("origin/")
        .map(|s| s.to_string())
        .or(Some(sym))
}

pub fn normalized_repo_execution_mode(raw: Option<&String>) -> String {
    match raw.map(|s| s.as_str().trim()).filter(|s| !s.is_empty()) {
        Some("apply_git") => "apply_git".into(),
        Some("apply_github") => "apply_github".into(),
        _ => "observe".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_execution_modes() {
        assert_eq!(
            normalized_repo_execution_mode(None),
            "observe",
        );
        let s = " apply_git ".to_string();
        assert_eq!(normalized_repo_execution_mode(Some(&s)), "apply_git");
        let g = "apply_github".to_string();
        assert_eq!(normalized_repo_execution_mode(Some(&g)), "apply_github");
        let bad = "merge_all".to_string();
        assert_eq!(normalized_repo_execution_mode(Some(&bad)), "observe");
    }

    #[test]
    fn auth_url_github_https() {
        let u =
            authenticated_github_https_url("https://github.com/acme/repo.git", Some("TokEn123"));
        assert!(u.starts_with("https://x-access-token:TokEn123@github.com/"));
        assert!(u.ends_with("/acme/repo.git"));

        assert_eq!(
            authenticated_github_https_url("https://github.com/a/b", Some("")),
            "https://github.com/a/b",
        );

        assert_eq!(
            authenticated_github_https_url("https://user:pwd@github.com/a/b.git", Some("ignored")),
            "https://user:pwd@github.com/a/b.git",
        );

        assert_eq!(
            authenticated_github_https_url("https://gitlab.com/x/y", Some("tok")),
            "https://gitlab.com/x/y",
        );
    }
}
