#!/usr/bin/env bash
# Shared bootstrap + helpers for autonomous worker loops.

worker_agent_load_global_paths() {
  CONFIG="${AI_AGENT_CONFIG:-/workspace/agent-config.json}"
  CTX="${AI_CONTEXT_PATH:-/workspace/context.json}"
  STATE="${AI_AGENT_STATE:-/persist/agent_loop_state.json}"
  OLLAMA="${OLLAMA_HOST:-http://host.docker.internal:11434}"
  POLL="${AI_AGENT_POLL_SECONDS:-45}"
  PENDING="${AI_AGENT_PENDING:-/persist/.agent_pending_ops}"
}

worker_agent_require_config_or_sleep() {
  if [[ ! -f "$CONFIG" ]]; then
    worker_agent_log "no $CONFIG — sleeping"
    exec sleep infinity
  fi

  model="$(jq -r '.model // "gemma4:e2b"' "$CONFIG")"
  sys_file="$(jq -r '.systemPromptPath // "/workspace/system-prompt.txt"' "$CONFIG")"
  local po="${AI_AGENT_POLL_SECONDS:-}"
  if [[ -z "$po" || "$po" == "null" ]]; then po="$(jq -r '.pollSeconds // empty' "$CONFIG")"; fi
  if [[ -n "$po" && "$po" != "null" && "$po" != "" ]]; then POLL="$po"; fi

  system_prompt=""
  if [[ -f "$sys_file" ]]; then system_prompt="$(cat "$sys_file")"; fi
}

worker_agent_log() {
  echo "[worker-agent] $*"
}

# Best-effort redaction before writing to context.json or host-mounted runtime log.
redact_for_context() {
  printf '%s' "$1" | sed -E \
    -e 's|https://oauth2:[^@[:space:]"]+@|https://oauth2:***@|g' \
    -e 's|https://x-access-token:[^@[:space:]"]+@|https://x-access-token:***@|g' \
    -e 's|Bearer[[:space:]]+[A-Za-z0-9._-]{8,}|Bearer ***|g' \
    -e 's|ghp_[A-Za-z0-9]{20,}|ghp_***|g' \
    -e 's|github_pat_[A-Za-z0-9_]+|github_pat_***|g' \
    -e 's|gho_[A-Za-z0-9]{20,}|gho_***|g' \
    -e 's|ghu_[A-Za-z0-9]{20,}|ghu_***|g' \
    -e 's|ghs_[A-Za-z0-9]{20,}|ghs_***|g' \
    -e 's|xox[baprs]-[A-Za-z0-9-]{10,}|xox-***|g'
}

worker_agent_append_host_runtime_log() {
  local line="$1"
  [[ -z "$line" ]] && return 0
  local f="${AI_REPO_RUNTIME_LOG:-}"
  [[ -z "$f" ]] && return 0
  line="$(redact_for_context "$line")"
  if [[ -f "$f" ]] && [[ -w "$f" ]]; then
    printf '[%s] %s\n' "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" "$line" >>"$f" || true
  fi
}

state_init() {
  if [[ ! -f "$STATE" ]]; then
    echo '{"oneShotDone":{},"cadenceLastBucket":{}}' >"$STATE"
  fi
}

oneshot_done() {
  local tid="$1"
  jq -e --arg id "$tid" '.oneShotDone[$id] == true' "$STATE" >/dev/null 2>&1
}

mark_oneshot() {
  local tid="$1"
  local tmp
  tmp="$(mktemp)"
  jq --arg id "$tid" '.oneShotDone[$id] = true' "$STATE" >"$tmp"
  mv "$tmp" "$STATE"
}

cadence_prev() {
  local tid="$1"
  jq -r --arg id "$tid" '.cadenceLastBucket[$id] // "none"' "$STATE"
}

set_cadence() {
  local tid="$1"
  local b="$2"
  local tmp
  tmp="$(mktemp)"
  jq --arg id "$tid" --argjson b "$b" '.cadenceLastBucket[$id] = $b' "$STATE" >"$tmp"
  mv "$tmp" "$STATE"
}

update_context_summary() {
  local summary="$1"
  summary="$(redact_for_context "$summary")"
  local tmp
  tmp="$(mktemp)"
  if [[ -f "$CTX" ]]; then
    jq --arg s "$summary" '.lastSummary = $s' "$CTX" >"$tmp"
  else
    jq -n --arg s "$summary" '{version:1,lastSummary:$s,goals:[],factsLearned:[],openQuestions:[],extra:{}}' >"$tmp"
  fi
  mv "$tmp" "$CTX"
}

append_fact() {
  local fact="$1"
  fact="$(redact_for_context "$fact")"
  local tmp
  tmp="$(mktemp)"
  if [[ ! -f "$CTX" ]]; then update_context_summary ""; fi
  jq --arg f "$fact" '.factsLearned += [$f]' "$CTX" >"$tmp"
  mv "$tmp" "$CTX"
}

commit_pending() {
  [[ -f "$PENDING" ]] || return 0
  while read -r line; do
    [[ -z "$line" ]] && continue
    local op tid b
    op="$(echo "$line" | cut -d' ' -f1)"
    tid="$(echo "$line" | cut -d' ' -f2)"
    if [[ "$op" == "os" ]]; then mark_oneshot "$tid"
    elif [[ "$op" == "cd" ]]; then
      b="$(echo "$line" | cut -d' ' -f3)"
      set_cadence "$tid" "$b"
    fi
  done <"$PENDING"
  rm -f "$PENDING"
}

collect_due() {
  local now titles any
  now="$(date -u +%s)"
  titles=""
  any=0
  state_init
  : >"$PENDING"

  local n shell_i
  n="$(jq '.tasks | length' "$CONFIG")"

  shell_i=0
  while [[ "$shell_i" -lt "$n" ]]; do
    local row kind tid title interval bucket prev
    row="$(jq -c ".tasks[$shell_i]" "$CONFIG")"
    kind="$(echo "$row" | jq -r '.schedule.kind')"
    tid="$(echo "$row" | jq -r '.id')"
    title="$(echo "$row" | jq -r '.title')"
    if [[ "$kind" == "oneShot" ]]; then
      if oneshot_done "$tid"; then
        shell_i=$((shell_i + 1))
        continue
      fi
      any=1
      titles+="- (one-shot) ${title}"$'\n'
      echo "os $tid" >>"$PENDING"
    elif [[ "$kind" == "cadence" ]]; then
      interval="$(echo "$row" | jq -r '.schedule.intervalSeconds')"
      bucket=$((now / interval))
      prev="$(cadence_prev "$tid")"
      if [[ "$prev" != "none" && "$prev" == "$bucket" ]]; then
        shell_i=$((shell_i + 1))
        continue
      fi
      any=1
      titles+="- (every ${interval}s) ${title}"$'\n'
      echo "cd $tid $bucket" >>"$PENDING"
    fi
    shell_i=$((shell_i + 1))
  done

  if [[ "$any" -eq 0 ]]; then
    rm -f "$PENDING"
    echo ""
    return 1
  fi
  printf '%s' "$titles"
}

worker_agent_invoke_ollama() {
  local user_msg="$1"
  local resp
  if ! resp="$(curl -sS --max-time 300 -X POST "$OLLAMA/api/chat" -H "Content-Type: application/json" -d "$(jq -n \
    --arg model "$model" \
    --arg sys "$system_prompt" \
    --arg user "$user_msg" \
    '{
      model: $model,
      stream: false,
      messages: [{role:"user", content: $user}]
    } + (if ($sys | length) > 0 then {system: $sys} else {} end)')")"; then
    return 1
  fi
  printf '%s' "$resp"
}
