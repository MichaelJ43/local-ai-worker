#!/usr/bin/env bash
set -euo pipefail
if [[ "${AI_AGENT_LOOP:-1}" == "1" ]]; then
  /opt/ai-worker/scripts/agent-loop.sh &
fi
exec sleep infinity
