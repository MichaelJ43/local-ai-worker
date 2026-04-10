# Local AI Worker — User guide

## What it does

- Runs **Ollama** via **Docker Compose** (bundled YAML + optional GPU file) or your own compose at the repo root.
- Manages **workers**: JSON config, optional **guardrail overrides**, **tasks** (one-shot or cadence).
- Prepares **per-worker storage**: context file on disk + Docker **volume** for long-term data, then can **start/stop/recreate** an **agent container**.
- **Agent loop** (in the worker image): background script calls **Ollama** on a poll interval, sends tasks + context, and updates `context.json` (requires a pulled model and running Ollama stack).
- **Guardrail enforcement (v1)**: `git` and `gh` in the container go through **`worker-guard`** (rate limits, branch/delete rules, merge policy, optional repo allow/deny via `AI_WORKER_REPO`). State lives under `/persist` on the worker volume; **audit** rows can be written to the **same** `audit.sqlite3` the app uses (mounted read/write into the container).
- Stores a **GitHub token** in **Windows Credential Manager** or **macOS Keychain**.
- Keeps a **GitHub audit** SQLite DB and an in-app **action log**.

## First-time setup

1. Install **Docker Desktop**, **Rust**, **Node.js**.
2. Build the agent image (used by worker containers):

   ```bash
   docker build -f docker/Dockerfile.worker -t local-ai-worker-agent:latest .
   ```

3. Start the app (`npm install` then `npm run tauri dev`).
4. **Ollama stack → Compose up** (pick GPU mode if appropriate).
5. Pull a model, e.g. `docker exec -it local-ai-ollama ollama pull gemma4:e2b`.
6. Save your **GitHub token** in the app.

## Workers

1. **Add worker** → **Save workers**.
2. **Prepare storage** — creates `context.json` under app data (`…/local-ai-worker/workers/<id>/`) and a Docker volume `local-ai-lt-…` unless you override paths in saved JSON (advanced).
3. **Start container** — runs `docker run` with:
   - Context file at `/workspace/context.json`
   - Materialized **`guardrails.effective.json`**, **`system-prompt.txt`**, **`agent-config.json`** (under `workers/<id>/` on the host)
   - Host **`audit.sqlite3`** mounted at `/persist/audit.sqlite3` (shared with the app)
   - Long-term volume at `/persist`
   - `OLLAMA_HOST` (per worker or default `http://host.docker.internal:11434`)
   - `GITHUB_TOKEN` if you saved a token
   - **`AI_WORKER_REPO=owner/repo`** (optional): required when **repository allowlist** is enforced so `git`/`gh` wrappers can validate scope.
4. **Recreate** = stop + start (fresh container, same mounts). Rebuild the image after pulling app updates that change `worker-guard` or the agent scripts.

Default image: `local-ai-worker-agent:latest` (override per worker as **Docker image**).

### Optional: disable the agent loop

The container entrypoint starts the loop unless you set **`AI_AGENT_LOOP=0`** (advanced / manual `docker run` only; the desktop app currently always enables the loop).

## Rules

Bundled **Git** domain guardrails live in `docs/rules/rules-tree.json`. Optional **Guardrail overrides** on each worker are deep-merged into **`guardrails.effective.json`** for prompts and for **`worker-guard`** enforcement inside the container.

## Data locations (typical Windows)

- App root: `%LOCALAPPDATA%\local-ai-worker\`
- Workers: `workers.json`, `workers/<id>/context.json`, `workers/<id>/guardrails.effective.json`, `workers/<id>/system-prompt.txt`, `workers/<id>/agent-config.json`, `compose/`, `audit.sqlite3`

## Audit

**Refresh audit log** reads `github_audit` rows (newest first). **`worker-guard`** may append rows when **`gh`** / **`git`** operations succeed or fail (according to `auditPolicy` in the rules tree).
