#!/usr/bin/env bash
# Export static HTML by snapshotting a running LocalAiWorker.Docs instance.
# Usage: DOCS_URL=http://127.0.0.1:5055 OUT_DIR=./static-out ./static-export.sh
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
URL="${DOCS_URL:-http://127.0.0.1:5055}"
OUT="${OUT_DIR:-$ROOT/docs-site/static-out}"
mkdir -p "$OUT/Download"
curl -fsSL "$URL/" -o "$OUT/index.html"
curl -fsSL -L "$URL/Download" -o "$OUT/Download/index.html" || curl -fsSL -L "$URL/Download/" -o "$OUT/Download/index.html"
for segment in GettingStarted Guide Architecture RepoAgentSandbox; do
  mkdir -p "$OUT/$segment"
  curl -fsSL -L "$URL/$segment" -o "$OUT/$segment/index.html"
done
mkdir -p "$OUT/images"
if [[ -d "$ROOT/docs/images" ]]; then
  cp -R "$ROOT/docs/images/." "$OUT/images/" || true
fi
echo "Static export written to $OUT"
