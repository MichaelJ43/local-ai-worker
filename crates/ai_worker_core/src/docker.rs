//! Docker CLI availability check (Docker Desktop).

use std::process::Command;

use crate::Result;

pub fn docker_cli_available() -> bool {
    Command::new("docker")
        .args(["info"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn docker_version_summary() -> Result<String> {
    let out = Command::new("docker")
        .args(["version", "--format", "{{.Server.Version}}"])
        .output()?;
    if !out.status.success() {
        return Err(crate::Error::Docker(
            "docker version failed — is Docker Desktop running?".into(),
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_check_does_not_panic() {
        let _ = docker_cli_available();
    }
}
