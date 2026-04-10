#!/usr/bin/env bash
set -euo pipefail
exec /opt/ai-worker/bin/worker-guard run-gh -- "$@"
