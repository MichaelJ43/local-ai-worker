#!/usr/bin/env bash
# Guarded repo tree mutations for repo-agent-loop (sourced, not executed).
# Requires: REPO_ROOT, CONFIG, EXEC_MODE, append_fact, worker_agent_append_host_runtime_log set by caller.

sandbox_policy_bool() {
  local key="$1"
  local v
  v="$(jq -r --arg k "$key" '(.sandboxPolicy // {})[$k] // false' "$CONFIG" 2>/dev/null)"
  [[ "$v" == "true" ]]
}

sandbox_policy_num() {
  local key="$1"
  local d="$2"
  jq -r --arg k "$key" --argjson d "$d" '(.sandboxPolicy // {})[$k] // $d' "$CONFIG" 2>/dev/null
}

sandbox_rel_path_ok() {
  local p="$1"
  local allow_git="$2"
  [[ -z "$p" ]] && return 1
  [[ "$p" == /* ]] && return 1
  [[ "$p" == *..* ]] && return 1
  [[ "$p" == *//* ]] && return 1
  if [[ "$allow_git" != "true" ]]; then
    case "$p" in
      .git | .git/*) return 1 ;;
    esac
  fi
  return 0
}

maybe_apply_repo_file_writes() {
  local blob="$1"
  [[ "$EXEC_MODE" == "observe" ]] && return 0
  if ! sandbox_policy_bool "fileWritesEnabled"; then
    return 0
  fi
  local max_file max_total
  max_file="$(sandbox_policy_num "patchMaxFileBytes" 393216)"
  max_total="$(sandbox_policy_num "patchMaxTotalBytes" 3145728)"
  local ar_type
  ar_type="$(echo "$blob" | jq -r '.fileWrites | type' 2>/dev/null || echo null)"
  if [[ "$ar_type" != "array" ]]; then
    return 0
  fi
  local n allow_git
  n="$(echo "$blob" | jq '.fileWrites | length')"
  [[ "$n" -eq 0 ]] && return 0
  allow_git="false"
  if sandbox_policy_bool "allowGitInternalsWrites"; then
    allow_git="true"
  fi

  local total=0 i=0
  while [[ "$i" -lt "$n" ]]; do
    local row op rel has_txt has_b64 txt b64 nbytes
    row="$(echo "$blob" | jq -c ".fileWrites[$i]")"
    op="$(echo "$row" | jq -r '.op // "write"')"
    rel="$(echo "$row" | jq -r '.path // empty')"
    rel="${rel#"${rel%%[![:space:]]*}"}"
    rel="${rel%"${rel##*[![:space:]]}"}"
    rel="${rel#/}"

    if ! sandbox_rel_path_ok "$rel" "$allow_git"; then
      append_fact "fileWrites[$i]: rejected path $rel"
      i=$((i + 1))
      continue
    fi

    if [[ "$op" == "delete" ]]; then
      local tgt="$REPO_ROOT/$rel"
      if [[ -f "$tgt" ]]; then
        rm -f "$tgt"
        append_fact "fileWrites[$i]: deleted $rel"
        worker_agent_append_host_runtime_log "deleted $rel"
      else
        append_fact "fileWrites[$i]: delete skip (not a file) $rel"
      fi
      i=$((i + 1))
      continue
    fi

    has_txt="$(echo "$row" | jq 'has("content")')"
    has_b64="$(echo "$row" | jq 'has("contentBase64")')"
    txt=""
    b64=""
    if [[ "$has_txt" == "true" ]]; then
      txt="$(echo "$row" | jq -r '.content // ""')"
    fi
    if [[ "$has_b64" == "true" ]]; then
      b64="$(echo "$row" | jq -r '.contentBase64 // empty')"
    fi
    if [[ -z "$txt" && -z "$b64" ]]; then
      append_fact "fileWrites[$i]: missing content/contentBase64 for $rel"
      i=$((i + 1))
      continue
    fi
    if [[ -n "$txt" && -n "$b64" ]]; then
      append_fact "fileWrites[$i]: both content and contentBase64; skipped $rel"
      i=$((i + 1))
      continue
    fi

    if [[ -n "$b64" ]]; then
      nbytes="$(printf '%s' "$b64" | base64 -d 2>/dev/null | wc -c | tr -d ' ')"
    else
      nbytes="${#txt}"
    fi
    if [[ "$nbytes" -gt "$max_file" ]]; then
      append_fact "fileWrites[$i]: content too large ($nbytes > $max_file) $rel"
      i=$((i + 1))
      continue
    fi
    if [[ "$((total + nbytes))" -gt "$max_total" ]]; then
      append_fact "fileWrites[$i]: cumulative patch budget exceeded ($total + $nbytes > $max_total)"
      i=$((i + 1))
      continue
    fi
    total=$((total + nbytes))

    local tgt="$REPO_ROOT/$rel"
    mkdir -p "$(dirname "$tgt")"
    local tmp="$tgt.__repo_agent.$$"
    if [[ -n "$b64" ]]; then
      if ! printf '%s' "$b64" | base64 -d >"$tmp" 2>/dev/null; then
        rm -f "$tmp"
        append_fact "fileWrites[$i]: base64 decode failed $rel"
        i=$((i + 1))
        continue
      fi
    else
      printf '%s' "$txt" >"$tmp"
    fi
    mv -f "$tmp" "$tgt"
    append_fact "fileWrites[$i]: wrote $rel ($nbytes bytes)"
    worker_agent_append_host_runtime_log "wrote $rel bytes=$nbytes"
    i=$((i + 1))
  done
}

maybe_apply_unified_diffs() {
  local blob="$1"
  [[ "$EXEC_MODE" == "observe" ]] && return 0
  if ! sandbox_policy_bool "applyUnifiedDiffsEnabled"; then
    return 0
  fi
  local ar_type
  ar_type="$(echo "$blob" | jq -r '.unifiedDiffs | type' 2>/dev/null || echo null)"
  if [[ "$ar_type" != "array" ]]; then
    return 0
  fi
  local n i=0
  n="$(echo "$blob" | jq '.unifiedDiffs | length')"
  [[ "$n" -eq 0 ]] && return 0

  local max_patch
  max_patch="$(sandbox_policy_num "patchMaxUnifiedDiffBytes" 1048576)"
  local GITBIN="${REALGIT:-/usr/bin/git}"

  while [[ "$i" -lt "$n" ]]; do
    local patch
    patch="$(echo "$blob" | jq -r --argjson idx "$i" '.unifiedDiffs[$idx] // empty')"
    if [[ -z "$patch" ]]; then
      i=$((i + 1))
      continue
    fi
    local psz
    psz="${#patch}"
    if [[ "$psz" -gt "$max_patch" ]]; then
      append_fact "unifiedDiffs[$i]: patch too large ($psz bytes)"
      i=$((i + 1))
      continue
    fi
    set +e
    chk="$(printf '%s' "$patch" | "$GITBIN" -C "$REPO_ROOT" apply --check --whitespace=nowarn 2>&1)"
    c1=$?
    if [[ "$c1" -ne 0 ]]; then
      append_fact "unifiedDiffs[$i]: git apply --check failed: $(echo "$chk" | head -c 500 | tr '\n' ' ')"
      worker_agent_append_host_runtime_log "unifiedDiff check fail idx=$i"
    else
      out="$(printf '%s' "$patch" | "$GITBIN" -C "$REPO_ROOT" apply --whitespace=nowarn 2>&1)"
      c2=$?
      append_fact "unifiedDiffs[$i]: git apply exit=$c2 $(echo "$out" | head -c 400 | tr '\n' ' ')"
      worker_agent_append_host_runtime_log "unifiedDiff applied idx=$i exit=$c2"
    fi
    set -e
    i=$((i + 1))
  done
}
