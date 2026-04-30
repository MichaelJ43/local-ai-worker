#!/usr/bin/env bash
set -euo pipefail

# GitHub CLI: use PAT from the environment; do not rely on `gh auth login` or
# credentials on disk. `gh` prefers GH_TOKEN over GITHUB_TOKEN; we mirror the
# app-injected GITHUB_TOKEN so non-interactive auth is consistent (fine-grained
# PATs work the same as classic when repo/API scopes match).
if [[ -n "${GITHUB_TOKEN:-}" ]]; then
  export GH_TOKEN="${GH_TOKEN:-$GITHUB_TOKEN}"
fi
# Isolate gh config dir so CLI auth comes from PAT env vars only — not ~/.config/gh
# from images or leaked volumes.
if [[ -z "${GH_CONFIG_DIR:-}" ]]; then
  export GH_CONFIG_DIR=/tmp/gh-worker-empty
  mkdir -p "$GH_CONFIG_DIR"
fi

if [[ "${AI_REPO_AGENT:-0}" == "1" ]]; then
  /opt/ai-worker/scripts/repo-agent-loop.sh &
elif [[ "${AI_AGENT_LOOP:-1}" == "1" ]]; then
  /opt/ai-worker/scripts/agent-loop.sh &
fi
exec sleep infinity
