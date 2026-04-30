#!/usr/bin/env bash
# Autonomous loop: reads agent-config.json + context.json, calls Ollama chat, updates context.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=docker/worker-agent-common.sh
source "$SCRIPT_DIR/worker-agent-common.sh"

worker_agent_load_global_paths
worker_agent_require_config_or_sleep

run_cycle() {
  local titles
  if ! titles="$(collect_due)"; then
    sleep "$POLL"
    return 0
  fi

  local ctx_excerpt=""
  if [[ -f "$CTX" ]]; then
    ctx_excerpt="$(jq -c . "$CTX" | head -c 12000)"
  fi

  local user_msg
  user_msg=$'Work through these worker tasks (be concise; note blockers in context):\n'"$titles"$'\nContext JSON:\n'"$ctx_excerpt"

  local resp
  if ! resp="$(worker_agent_invoke_ollama "$user_msg")"; then
    worker_agent_log "ollama request failed"
    rm -f "$PENDING"
    sleep "$POLL"
    return 0
  fi

  local content
  content="$(echo "$resp" | jq -r '.message.content // empty')"
  if [[ -z "$content" ]]; then
    worker_agent_log "empty model response"
    rm -f "$PENDING"
    sleep "$POLL"
    return 0
  fi

  commit_pending
  update_context_summary "$content"
  append_fact "$(echo "$content" | head -c 400)"
  worker_agent_log "cycle ok (${#content} chars)"
  sleep "$POLL"
}

worker_agent_log "starting legacy text loop model=$model poll=${POLL}s"
while true; do
  run_cycle || true
done
