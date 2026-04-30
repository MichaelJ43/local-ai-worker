#!/usr/bin/env bash
# Pure jq checks for repo-agent model JSON fixtures (runs in CI on Ubuntu).
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
command -v jq >/dev/null 2>&1 || {
  echo "jq is required" >&2
  exit 1
}

VALID="$ROOT/test/fixtures/repo-agent-model-valid.json"
INV="$ROOT/test/fixtures/repo-agent-model-invalid.txt"

jq -e . "$VALID" >/dev/null
clen="$(jq '.commands | length' "$VALID")"
[[ "$clen" == "2" ]] || {
  echo "expected 2 commands" >&2
  exit 1
}
jq -e '.commands[0].argv[0] == "git"' "$VALID" >/dev/null

if jq -e . "$INV" >/dev/null 2>&1; then
  echo "expected invalid fixture to fail jq" >&2
  exit 1
fi

echo "repo-agent fixtures OK"
