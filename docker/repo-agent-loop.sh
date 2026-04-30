#!/usr/bin/env bash
# Repo-backed autonomous loop: scripted repo facts + structured JSON proposing git/gh argv; optional execution.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=docker/worker-agent-common.sh
source "$SCRIPT_DIR/worker-agent-common.sh"
# shellcheck source=docker/repo_sandbox_writes.sh
source "$SCRIPT_DIR/repo_sandbox_writes.sh"

worker_agent_load_global_paths
worker_agent_require_config_or_sleep

REPO_ROOT="${REPO_ROOT:-/workspace/repo}"
EXEC_MODE="${REPO_EXECUTION_MODE:-observe}"
REALGIT="${REAL_GIT:-/usr/bin/git}"

repo_observation_block() {
  local out=""
  if [[ ! -d "$REPO_ROOT" ]]; then
    printf '%s' "(REPO_ROOT not a directory: $REPO_ROOT)"
    return 0
  fi
  out+="=== git status -sb (read-only via $REALGIT) ==="$'\n'
  out+="$("$REALGIT" -C "$REPO_ROOT" status -sb 2>&1 | head -c 4000)"$'\n'
  out+="=== git diff --stat ==="$'\n'
  out+="$("$REALGIT" -C "$REPO_ROOT" diff --stat 2>&1 | head -c 4000)"$'\n'
  out+="=== git log -5 --oneline ==="$'\n'
  out+="$("$REALGIT" -C "$REPO_ROOT" log -5 --oneline 2>&1 | head -c 2000)"$'\n'
  printf '%s' "$out"
}

json_instructions() {
  cat <<'JSON'
Respond with ONLY valid JSON on a single line (no markdown fences). Shape:
{"reflection":"brief summary","commands":[{"argv":["git","status"]}],"fileWrites":[{"op":"write","path":"relative/path.txt","content":"..."}],"unifiedDiffs":[]}
- commands: optional array of {argv} for guarded "git" or "gh" only (tiers apply_git / apply_github).
- fileWrites: optional; each entry: op "write" (default) or "delete", path relative to repo root (no .., no leading /). Use "content" (string) OR "contentBase64" (not both). Applied only if sandboxPolicy.fileWritesEnabled is true and execution tier is not observe.
- unifiedDiffs: optional array of unified diff strings; applied with real git apply only if sandboxPolicy.applyUnifiedDiffsEnabled is true and tier is not observe.
Empty arrays are fine. Paths must stay within the working tree; .git/* writes are blocked unless sandboxPolicy.allowGitInternalsWrites is true.
JSON
}

maybe_execute_commands_from_json() {
  local blob="$1"
  local cmds_len
  if ! cmds_len="$(echo "$blob" | jq -e '.commands | length')"; then
    return 0
  fi
  local idx=0 out_all=""
  while [[ "$idx" -lt "$cmds_len" ]]; do
    mapfile -t argv < <(echo "$blob" | jq -r ".commands[$idx].argv[]?")
    idx=$((idx + 1))
    if [[ "${#argv[@]}" -eq 0 ]]; then
      continue
    fi
    local bin="${argv[0]}"
    if [[ "$bin" != "git" && "$bin" != "gh" ]]; then
      local skipped
      printf -v skipped '%q ' "${argv[@]}"
      append_fact "repo-agent skipped non-git/gh: ${skipped%" "}"
      continue
    fi

    local cmd_disp=""
    printf -v cmd_disp '%q ' "${argv[@]}"
    append_fact "repo-agent proposed argv: ${cmd_disp%" "}"

    if [[ "$EXEC_MODE" == "observe" ]]; then
      append_fact "observe: argv not executed ($bin)"
      continue
    fi
    if [[ "$EXEC_MODE" == "apply_git" && "$bin" != "git" ]]; then
      append_fact "apply_git tier: only git argv executed; skipped: $bin (use apply_github for gh)"
      continue
    fi

    if [[ "$bin" == "git" ]]; then
      local has_c=0 j
      for ((j = 1; j < ${#argv[@]}; j++)); do
        if [[ "${argv[$j]}" == "-C" ]]; then
          has_c=1
          break
        fi
      done
      if [[ "$has_c" -eq 0 ]]; then
        argv=(git -C "$REPO_ROOT" "${argv[@]:1}")
      fi
    fi

    local run_out run_code
    set +e
    run_out="$("${argv[@]}" 2>&1)"
    run_code=$?
    set -e
    out_all+=$'\n'"$run_out"
    append_fact "$(printf 'repo-agent exec %s exit=%s head=%s' "$bin" "$run_code" "$(echo "$run_out" | head -c 800 | tr '\n' ' ')")"
  done
  [[ -n "$out_all" ]] && printf '%s' "$out_all"
}

maybe_run_allowed_test_profiles() {
  if [[ "$EXEC_MODE" == "observe" ]]; then
    return 0
  fi
  if [[ ! -d "$REPO_ROOT" ]]; then
    return 0
  fi
  local ar_type
  ar_type="$(jq -r '.allowedTestProfiles | type' "$CONFIG")"
  if [[ "$ar_type" != "array" ]]; then
    return 0
  fi
  local n
  n="$(jq '.allowedTestProfiles | length' "$CONFIG")"
  if [[ "$n" -eq 0 ]]; then
    return 0
  fi
  local i=0
  while [[ "$i" -lt "$n" ]]; do
    local cmd
    cmd="$(jq -r ".allowedTestProfiles[$i].command // empty" "$CONFIG")"
    cmd="${cmd#"${cmd%%[![:space:]]*}"}"
    cmd="${cmd%"${cmd##*[![:space:]]}"}"
    mapfile -t targv < <(jq -r ".allowedTestProfiles[$i].argv[]?" "$CONFIG" 2>/dev/null || true)
    if [[ -z "$cmd" ]]; then
      append_fact "allowedTestProfiles[$i]: empty command skipped"
      i=$((i + 1))
      continue
    fi
    if [[ ! "$cmd" =~ ^[A-Za-z0-9][A-Za-z0-9._+-]*$ ]]; then
      append_fact "allowedTestProfiles[$i]: invalid command name, skipped: $cmd"
      i=$((i + 1))
      continue
    fi
    if ! command -v "$cmd" >/dev/null 2>&1; then
      append_fact "allowedTestProfiles[$i]: $cmd not found on PATH"
      worker_agent_append_host_runtime_log "test profile [$i]: $cmd not on PATH"
      i=$((i + 1))
      continue
    fi
    local realcmd
    realcmd="$(command -v "$cmd")"
    local out code
    set +e
    out="$(cd "$REPO_ROOT" && "$realcmd" "${targv[@]}" 2>&1 | head -c 12000)"
    code=$?
    set -e
    append_fact "test profile [$i] $cmd exit=$code head=$(echo "$out" | head -c 800 | tr '\n' ' ')"
    worker_agent_append_host_runtime_log "test profile [$i] $cmd exit=$code"
    i=$((i + 1))
  done
}

run_cycle_repo() {
  local titles
  if ! titles="$(collect_due)"; then
    sleep "$POLL"
    return 0
  fi

  local ctx_excerpt=""
  if [[ -f "$CTX" ]]; then
    ctx_excerpt="$(jq -c . "$CTX" | head -c 12000)"
  fi

  local obs
  obs="$(repo_observation_block)"

  local instr
  instr="$(json_instructions)"

  local user_msg
  user_msg=$'Work through these worker tasks:\n'"$titles"$'\n'
  user_msg+=$'Current context JSON:\n'"$ctx_excerpt"$'\n'
  user_msg+=$'Repo observations (read-only):\n'"$obs"$'\n'
  user_msg+=$'Execution mode: '"$EXEC_MODE"$' — observe = no execution; apply_git = guarded git only; apply_github = guarded git + gh. File writes / unified diffs run before git/gh commands when enabled in sandboxPolicy. Test profiles run last.\n'
  user_msg+="$instr"

  local resp
  if ! resp="$(worker_agent_invoke_ollama "$user_msg")"; then
    worker_agent_log "ollama request failed"
    append_fact "repo-agent: Ollama request failed"
    worker_agent_append_host_runtime_log "ollama request failed"
    rm -f "$PENDING"
    sleep "$POLL"
    return 0
  fi

  local content
  content="$(echo "$resp" | jq -r '.message.content // empty')"
  if [[ -z "$content" ]]; then
    worker_agent_log "empty model response"
    append_fact "repo-agent: empty model response"
    worker_agent_append_host_runtime_log "empty model response"
    rm -f "$PENDING"
    sleep "$POLL"
    return 0
  fi

  local content_clean
  content_clean="$(printf '%s' "$content" | sed '/^```/d')"

  if ! echo "$content_clean" | jq -e . >/dev/null 2>&1; then
    commit_pending
    update_context_summary "$content"
    append_fact "repo-agent: model output was not parseable JSON; stored as lastSummary only"
    worker_agent_append_host_runtime_log "model output not valid JSON"
    worker_agent_log "non-json model output"
    sleep "$POLL"
    return 0
  fi

  local reflection
  reflection="$(echo "$content_clean" | jq -r '.reflection // ""')"
  commit_pending
  update_context_summary "$reflection"
  append_fact "$(echo "$reflection" | head -c 400)"
  maybe_apply_repo_file_writes "$content_clean" || true
  maybe_apply_unified_diffs "$content_clean" || true
  maybe_execute_commands_from_json "$content_clean" || true
  maybe_run_allowed_test_profiles || true
  worker_agent_log "repo cycle ok"
  sleep "$POLL"
}

worker_agent_log "starting repo agent loop REPO_ROOT=$REPO_ROOT mode=$EXEC_MODE model=$model poll=${POLL}s"

while true; do
  run_cycle_repo || true
done
