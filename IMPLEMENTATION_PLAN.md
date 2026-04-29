# Implementation Plan: Hybrid Local (Ollama) + Cursor SDK Worker

This document captures the agreed architecture for **local-ai-worker**: automate repetitive repository maintenance (security fixes, bugs, dependency updates) with a **two-tier model**—cheap local inference first, cloud escalation when needed.

---

## Goals

- Run **locally** using **Ollama** for common, shallow fixes (e.g., bumping a package version that resolves a vulnerability in one pass).
- Fall back to **Cursor’s coding agent via the TypeScript SDK** when local attempts fail or exceed bounded time—trading privacy/spend for stronger models and higher context.
- Move toward **self-maintaining, repeatedly updated projects** driven by your own triggers (schedulers, webhooks, CLI), not manual sessions in the Cursor IDE.

Non-goals for this plan: replacing Cursor with Ollama entirely for hard tasks; using the Cursor SDK for non-coding workloads (e.g., games).

---

## Rationale

| Layer | Role |
|-------|------|
| **Ollama** | Fast iteration, no cloud inference cost for straightforward patches; keeps sensitive repos offline when successful. |
| **Cursor SDK** | Same coding-agent stack as Cursor; strong at multi-file reasoning and ambiguous failures after local limits are hit. |

Local models are **weaker and lower-context** than Cursor’s cloud agents; the hybrid avoids wasting cloud quota on fixes Ollama can complete alone.

---

## High-Level Architecture

```
Trigger (cron / webhook / CLI)
       │
       ▼
┌──────────────────┐
│ Prepare workspace │  clone / sync repo, gather task context (CVE, issue, file hints)
└─────────┬────────┘
          │
          ▼
┌──────────────────┐     success + verified
│ Try Ollama loop   │ ─────────────────────────► Done (commit / PR / notify)
│ (bounded time)    │
└─────────┬────────┘
          │ failure / timeout / unverified
          ▼
┌──────────────────┐
│ Cursor SDK run    │  agent on repo with rich handoff payload
└─────────┬────────┘
          │
          ▼
       Done / failed (alert)
```

---

## Escalation Rules (To Implement)

Define explicit transitions so behavior is predictable and testable.

1. **Success path (stay local)**  
   - Ollama proposes a patch within **T_local** (wall-clock or max attempts).  
   - **Verification passes**: e.g., targeted tests, build, linter, security scan—or a minimal “same issue key no longer present” check suitable for the task type.

2. **Escalate to Cursor**  
   - Wall-clock **timeout** exceeded without verified fix.  
   - Repeated identical failures (same error after apply).  
   - Patch apply failures or inconsistent edits Ollama cannot recover from within limits.  
   - Optional: confidence/heuristic threshold if you add scoring later.

3. **Never escalate blindly**  
   Always attach structured context (see Handoff Contract) so the Cursor agent is not cold-starting.

---

## Verification

Verification is **task-type dependent**. Examples:

- **Dependency bump:** lockfile updated, `npm ci` / `pip install` equivalent succeeds, unit tests or smoke tests pass.
- **Security scanner:** re-run scanner or equivalent rule; issue cleared or severity reduced per policy.

Implement a small **plugin or strategy interface** per task kind (`dependency_fix`, `static_analysis_fix`, …) so new task types do not hard-code one global check.

---

## Handoff Contract (Ollama → Cursor)

When escalating, include at minimum:

| Field | Purpose |
|-------|---------|
| Task description | Original instruction (“fix CVE-… on repo X”). |
| Repo identity | Remote URL, branch, commit SHA before agent run. |
| Ollama transcript | Prompts/responses or summarized attempts (truncate if huge). |
| Failed patches / diffs | What was tried and rejected or reverted. |
| Logs | Build/test/scanner output relevant to failure. |
| Environment | Language/runtime versions if material. |

Store artifacts under a run ID for audit and reproducibility.

---

## Configuration (Suggested)

| Key | Purpose |
|-----|---------|
| `OLLAMA_*` | Base URL, model name(s), optional timeouts per phase. |
| `CURSOR_API_KEY` | From [Cursor Dashboard → Integrations](https://cursor.com/dashboard/integrations). |
| `LOCAL_PHASE_TIMEOUT_MS` / `LOCAL_MAX_ATTEMPTS` | Bounds for Ollama phase. |
| `CURSOR_PHASE_TIMEOUT_MS` | Optional cap for cloud agent run (aligned with SDK cancellation). |

Secrets must never be logged; mask in transcripts sent to notifications.

---

## Security & Operations

- **Local-first**: repos that must never leave the machine should be flagged so the worker **skips Cursor** entirely or uses a separate policy profile.
- **Cursor path**: understand Cursor’s terms for sending repo/context to the hosted agent; document for operators.
- **Idempotency**: runs should be keyed by task + repo revision to avoid duplicate PRs or conflicting fixes.
- **Rate limits**: Cursor APIs enforce per-team limits; backoff on HTTP 429 per Cursor docs.

---

## Implementation Phases

### Phase 1 — Foundations

- Single-repo, single-task CLI entrypoint: clone/sync workspace, run one Ollama “attempt” with a fixed prompt template for a narrow task (e.g., semver bump from advisory input).
- Pluggable **verifier** hook returning pass/fail + logs.
- Structured logging and run IDs.

### Phase 2 — Local loop & escalation

- Bounded loop with timeout and max attempts; aggregate transcript for handoff.
- Integrate **Cursor TypeScript SDK** for escalation path only: spawn agent with workspace + handoff payload (follow [cookbook quickstart](https://github.com/cursor/cookbook) and [SDK docs](https://cursor.com/docs/sdk/typescript)).
- Notification hook on success/failure (exit codes, webhooks, or stdout JSON).

### Phase 3 — Production hardening

- Multiple task strategies; queue or scheduler integration.
- Policy matrix: which repos allow Cursor escalation.
- Metrics: local success rate vs escalate rate, time spent per tier.

---

## References

- Cursor cookbook & SDK examples: `https://github.com/cursor/cookbook`  
- Cursor APIs overview: `https://cursor.com/docs/api`  
- TypeScript SDK: `https://cursor.com/docs/sdk/typescript`

---

## Open Questions

- Canonical PR strategy: branch naming, push credentials, CI requirements before merge.
- Whether to persist intermediate workspace tarballs for Cursor-side debugging.
- Model selection per task type on Ollama vs single default.

---

*Document generated from design discussion; revise as the codebase evolves.*
