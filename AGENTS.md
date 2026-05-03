# AGENTS.md — AI / agent context for this repository

This file is the **primary onboarding document** for coding agents (and humans) starting a new chat or task. It summarizes layout, architecture, boundaries, and where to change what. For user-facing usage, see **`README.md`** and **`docs/USER_GUIDE.md`**. Original product intent lives in **`project_prompt.txt`** (may not match every detail of the current codebase).

---

## What this project is

**Local AI Worker** is a **Tauri 2** desktop app: a **Vite** frontend (`src/`) talks to a **Rust** backend (`src-tauri/`) that persists workers, manages **secrets** (OS keychain + index file), drives **Docker** (Ollama compose + per-worker agent containers), and embeds shared logic in **`ai_worker_core`**. Agent runtimes use a **bundled worker image** (`docker/`) with an **agent loop** that reads `agent-config.json`, schedules **cadence** vs **one-shot** tasks, calls **Ollama**, and updates **context JSON**.

---

## Tech stack

| Layer | Stack |
|--------|--------|
| Desktop shell | Tauri 2, Rust stable |
| UI | Vanilla HTML/CSS/JS, Vite 5, `@tauri-apps/api` v2 |
| Shared logic | `ai_worker_core` crate (rules, context, audit, Ollama client, hardware, guard exec, worker config) |
| Worker container | Docker image built from `docker/` (bash agent loop, git/gh guard wrappers) |
| Persistence (app) | `workers.json`, **`llm_sources.json`** (Ollama + Cursor catalog for escalation tiers), `secret_keys.json`, `pending_restore_prompt.json`, `audit.sqlite3`, per-worker dirs under app data |
| CI | GitHub Actions: path-filtered frontend build, Playwright e2e, Rust test + clippy (multi-OS) |

---

## Top-level layout

```
.github/               # CI (ci.yml, release.yml, version-bump.yml), PR template
crates/
  ai_worker_core/      # Library: rules, context, audit, docker helpers, worker_config, worker-guard binary
  ai_worker_hybrid/     # Bounded Ollama + Cursor SDK bridge types (no Tauri); used by src-tauri
cursor-agent-bridge/   # Node: @cursor/sdk CLI (cli.mjs); npm ci here before hybrid escalation
docker/                # Dockerfile.worker, agent-loop.sh, repo-agent-loop.sh, worker-agent-common.sh, entrypoints, git/gh wraps
docs-site/             # ASP.NET Core docs + downloads (GHCR local-ai-worker-docs; Terraform infra/terraform/docs-site-*)
docs/                  # USER_GUIDE, COMPOSE_WORKERS, architecture, rules/rules-tree.json, schemas/
IMPLEMENTATION_PLAN.md # Architecture notes: hybrid Ollama + Cursor SDK
scripts/               # bump-version.mjs, inject-updater-endpoint.mjs
src/                   # Vite app: index.html, main.js, styles.css, updater.js → dist/
src-tauri/             # Tauri crate: lib.rs, compose.rs, secrets.rs, worker_docker.rs, tauri.conf.json, capabilities/, resources/compose/
test/                  # Rust integration tests (ai_worker_integration); playwright tests under test/playwright/
Cargo.toml             # Workspace root (version in [workspace.package])
package.json           # npm scripts; version should match workspace
playwright.config.js   # e2e: build + preview on port 4173
vite.config.js
docker-compose.yml     # Optional root compose (Ollama); app also bundles compose under src-tauri/resources/compose/
```

Generated / not primary sources of truth for agents: `src/dist/`, `target/`, `node_modules/`, `src-tauri/gen/` (Tauri-generated schemas; regenerate via Tauri CLI when needed).

---

## Architecture boundaries (dependency direction)

1. **`crates/ai_worker_core`** — **No Tauri**. Pure Rust. Used by `src-tauri` and `test`. Embeds `docs/rules/rules-tree.json` via `include_str!`. Adding a domain means editing that JSON and any Rust validation in `worker_config` / `rules`.
2. **`crates/ai_worker_hybrid`** — **No Tauri**. Bounded Ollama loop + verifier trait + Node `cursor-agent-bridge` stdin/stdout JSON. Consumed by `src-tauri` hybrid commands only.
3. **`src-tauri`** — **Tauri commands**, filesystem, keyring, Docker CLI. Depends on `ai_worker_core` and `ai_worker_hybrid`. **`src-tauri/src/lib.rs`** registers all `#[tauri::command]` handlers and `RunEvent::Exit` (session snapshot). **`secrets.rs`** = KV secret names + keychain entries. **`worker_docker.rs`** = container lifecycle + `env_from_secrets` → `-e` flags. **`hybrid.rs`** = host-side hybrid run (`hybrid_run_worker`, `hybrid_bridge_status`).
4. **`src/` (frontend)** — **No direct filesystem**. Uses `invoke("commandName", { ... })` only. CamelCase in JSON matches serde `rename_all = "camelCase"` on Rust structs.
5. **`docker/`** — Agent image and loop; must stay consistent with **`WorkerDefinition`** / `agent-config.json` shape materialized in `worker_docker.rs` (`materialize_worker_runtime`).

**Rule of thumb:** schema changes to workers require **`WorkerDefinition`** in Rust, **`docs/schemas/worker-definition.schema.json`**, frontend **`workerTemplate` / renderers**, and any **`test/`** struct literals.

---

## App data directory (runtime)

Resolved in Rust as: `dirs::data_local_dir()/local-ai-worker` (see `app_dir()` in `src-tauri/src/lib.rs`).

Typical contents:

| File / dir | Role |
|------------|------|
| `workers.json` | Serialized `Vec<WorkerDefinition>` |
| `llm_sources.json` | Serialized `Vec<LlmSourceDefinition>` (tagged **`ollama`** / **`cursor`**); worker **`escalationPath`** references source ids |
| `secret_keys.json` | Index of secret **names** (values in OS keychain) |
| `pending_restore_prompt.json` | Last-exit enabled snapshot for “Welcome back” UI |
| `audit.sqlite3` | GitHub mutation audit log |
| `workers/<id>/` | `context.json`, materialized `guardrails.effective.json`, `system-prompt.txt`, `agent-config.json`; host git checkout at `checkout/` when `hybridOptions.repoUrl` is set (bind-mounted `/workspace/repo`); optional **`repo-agent-runtime.log`** (redacted one-line events alongside context) |

---

### Repo-backed autonomous Docker worker (`AI_REPO_AGENT=1`)

When a worker saves a non-empty **`hybridOptions.repoUrl`**, Rust **`worker_repo_checkout`** clones or syncs into **`workers/<id>/checkout`**, **`worker_docker`** bind-mounts it at **`/workspace/repo`**, and the container entrypoint runs **`docker/repo-agent-loop.sh`** instead of the legacy **`agent-loop.sh`**.

- Facts for the prompt are assembled from **explicit read-only `/usr/bin/git`** commands (`status`, `diff --stat`, `log`); **`git/gh` mutation** proposals use guarded **`PATH`** wrappers (**`worker-guard`** behind **`git-wrap.sh`** / **`gh-wrap.sh`**).
- **`repoExecutionMode`**: **`observe`** (record only); **`apply_git`** (exec **`git`** proposals only); **`apply_github`** (exec **`git` + `gh`**). Structured JSON may also include **`fileWrites`** / **`unifiedDiffs`** when enabled in **`sandboxPolicy`** (**`RepoSandboxPolicy`**, persisted + materialized).
- Persisted **`allowedTestProfiles`** runs whitelisted **`command` + `argv`** after guarded commands each cycle when not **`observe`**.
- **`workers/<id>/repo-agent-runtime.log`**: appended from the loop for errors / test-runner notes (patterns like tokens and embedded GitHub HTTPS credentials in URLs are stripped before persistence).

Network and patch policy details: **`docs/REPO_AGENT_SANDBOX.md`**.

## Core concepts

### Hardware / GPU hints

`crates/ai_worker_core/src/hardware.rs` **`probe_system`** sets **`hasDiscreteGpuHint`** and **`notes`** using, in order: **`nvidia-smi -L`** (with optional VRAM query); else **macOS** Apple Silicon (`aarch64`) or `system_profiler SPDisplaysDataType -json`; else **Linux** `lspci` for VGA/3D/display lines; else **Windows** `Get-CimInstance Win32_VideoController`. **`nvidia_smi_available`** is shared with Tauri **`compose`** for the Ollama GPU compose file.

### Worker definition (`WorkerDefinition`)

- **`maintenanceDomain`** — Key into `rules-tree.json` `domains` (e.g. `git`). Drives guardrails + prompt section.
- **`escalationPath`** — Ordered list of **`llm_sources.json`** tier ids for model resolution and hybrid escalation (must include at least one **Ollama** tier to enable Docker agents; Cursor-before-Ollama ordering is rejected).
- **`tasks`** — Each has `schedule`: **`oneShot`** or **`cadence`** with **`intervalSeconds`** (see `docker/agent-loop.sh` for due logic).
- **`envFromSecrets`** — Maps **secret key** (KV store name) → **container env var**. If `GITHUB_TOKEN` not mapped, legacy/`github_token` secret still injected when present.
- **`hybridOptions`** — Optional host-side **bounded Ollama + Cursor SDK** escalation (see `crates/ai_worker_hybrid`, `cursor-agent-bridge/cli.mjs`). Uses keychain secret (default name `cursor_api_key`) for `CURSOR_API_KEY` when invoking Node; **`repoUrl`** + **`startingRef`** drive host **checkout** (`workers/<id>/checkout`) mounted at **`/workspace/repo`** when the repo agent loop is enabled, and are echoed into **`system-prompt.txt`**.
- **`repoExecutionMode`** — Autonomy tier when the repo-backed loop runs: **`observe`**, **`apply_git`**, or **`apply_github`** (see AGENTS subsection “Repo-backed autonomous”).
- **`allowedTestProfiles`** — Optional list of **`{ command, argv }`** profiles (bare `PATH` command names) invoked from **`REPO_ROOT`** after model-proposed guarded commands each cycle (**`apply_git` / `apply_github`** only).
- **`dockerNetwork`** — Optional **`docker run --network`** name (validated). Empty omits **`--network`** (daemon bridge). Tradeoffs (**`bridge`**, **`none`**, internal bridge) → **`docs/REPO_AGENT_SANDBOX.md`**.
- **`repoSandboxPolicy`** — Caps and toggles for model **`fileWrites`** and **`git apply`** unified diffs; defaults disable tree mutation via those channels.
- **`workerPrompt`** — Optional free-text block merged into **`system-prompt.txt`** after domain guardrails whenever the Docker runtime is materialized.
- **`enabled`** — UI/scheduling intent; reopen prompt uses last saved enabled flags on app exit.

### Secrets

- **KV store:** `secret_keys_list` / `secret_set` / `secret_delete`; values in keychain as `secret:<name>`; legacy GitHub entry migrated to `github_token` when listing.
- **GitHub helpers:** `set_github_token` / `delete_github_token` still exist; forward to KV + clear legacy.

### Rules

- Source: **`docs/rules/rules-tree.json`**. Exposed to UI via **`rules_domains_list`** for domain picker + help copy.

### Docker

- **Compose:** `src-tauri/src/compose.rs` + bundled YAML in **`src-tauri/resources/compose/`** copied to app data before `docker compose`. The app **automatically runs `compose up/down`** when persisted workers imply a loopback Ollama stack (see `persist_llm` apply-runtime), so the UI no longer exposes manual Compose actions on Overview.
- **Worker containers:** `worker_docker.rs` — `prepare_worker_storage`, `worker_start`, logs, etc. Image default **`local-ai-worker-agent:latest`** unless a worker overrides `dockerImage`. **`docker/entrypoint.sh`** copies **`GITHUB_TOKEN` → `GH_TOKEN`** (when set) and fixes **`GH_CONFIG_DIR`** to an ephemeral directory so **`gh`** uses the PAT in the environment—not interactive login or disk hosts.

---

## Frontend (`src/`)

- **`main.js`** — Navigation views, **collapsed worker cards** (summary row Enable / Edit / Remove), per-worker save/discard, LLM catalog editor (**default model dropdown** per Ollama source, populated asynchronously by `ollama_list_models`), secrets table, diagnostics, modals (domain/tasks help, session restore). **`save_workers`** and session restore **`session_resolve_restore`** return **`runtimePending`** and apply Docker/runtime in the background while the UI listens for **`runtime-phase`**, **`runtime-finished`**, and **`runtime-error`** events and shows a **`#runtime-banner`** banner.
- **`index.html`** — Shell: sidebar nav (Overview, LLM sources, Workers, Secrets, Diagnostics), Compose stack **status hint** line on Overview.
- **`styles.css`** — Layout and component styles.
- **`updater.js`** — Update check scheduler + Tauri updater plugin.

**E2E:** `test/playwright/smoke.spec.js` — assumes nav shell (e.g. Workers is a **button**, Workers **h2** is in a tab). Run `npm run test:e2e` (starts preview on 4173).

---

## Tauri command surface (reference)

Registered in **`src-tauri/src/lib.rs`** `generate_handler!`:

- Workers: `get_workers`, `save_workers`, `delete_worker` (disabled workers only — tears down Docker/workspace), `worker_storage_prepare`, `worker_docker_start` / `stop` / `recreate` / `status` / `logs`
- LLM catalog: `get_llm_sources`, `save_llm_sources` (persists **`llm_sources.json`** beside `workers.json`; migration merges legacy worker Ollama/Cursor hints)
- Hybrid (host): `hybrid_bridge_status`, `hybrid_run_worker` (bounded local Ollama + `@cursor/sdk` via `cursor-agent-bridge`; requires Node on PATH + `npm ci` in `cursor-agent-bridge/`)
- Secrets: `secret_keys_list`, `secret_set`, `secret_delete`, `github_token_configured`, `set_github_token`, `delete_github_token`
- Environment: `docker_status`, `hardware_profile`, `ollama_list_models`, `ollama_stack_gpu_hint` (also used by the Overview Compose status line). Host-side `ollama_stack_up` / `down` / `status` remain callable for diagnostics / automation even though the Workers UI relies on automatic stack management after saves.
- Rules / UX: `assemble_prompt_preview`, `rules_domains_list`, `session_peek_pending_restore`, `session_resolve_restore`
- Other: `audit_record_github`, `audit_recent_github`, `app_log_lines`, `open_external_url`

Adding a command: implement in `lib.rs` (or module), register in `generate_handler!`, call from frontend with matching argument names (camelCase for serde).

---

## Testing

| Command | Scope |
|---------|--------|
| `cargo test --workspace` | Rust unit + integration (`test/tests/`, `ai_worker_core` tests) |
| `cargo clippy --workspace -- -D warnings` | Lint (CI uses this) |
| `npm run build` | Vite production build |
| `npm run test:e2e` | Playwright against built preview |
| `bash test/scripts/repo_agent_fixture_test.sh` (or `npm run test:fixtures`) | Validates repo-agent JSON fixtures with `jq` |

CI path filters (`.github/workflows/ci.yml`): changes under `src/`, `playwright`, `src-tauri`, `crates`, etc., gate relevant jobs.

---

## Versioning and release

- **Bump:** `node scripts/bump-version.mjs patch|minor|major` updates root `Cargo.toml` `[workspace.package].version`, `package.json`, `crates/ai_worker_core/Cargo.toml`, `tauri.conf.json`, lockfiles.
- **Workflows:** `version-bump.yml` (merge to main), `release.yml` (tauri-action, installers, updater artifacts). See **README** for secrets (`RELEASE_PUSH_TOKEN`, `TAURI_SIGNING_PRIVATE_KEY`).

---

## Security / secrets (for agents)

- **Never commit** private signing keys (e.g. `src-tauri/.updater-signing.key`), tokens, or real `workers.json` from user machines.
- **`.gitignore`** already excludes common key paths; verify before `git add -A`.

---

## When you change X, also check Y

| Change | Also update |
|--------|-------------|
| `WorkerDefinition` / LLM catalog fields | `docs/schemas/worker-definition.schema.json`, `docs/schemas/llm-sources.schema.json`, `main.js` (`workerTemplate`, render, escalation + LLM sources UI), `test/tests/*.rs` literals, `worker_docker` / agent-config materialization if needed |
| Rules domains / guardrails | `docs/rules/rules-tree.json`, `rules_domains_list` consumers, USER_GUIDE if user-visible |
| Agent task schedule semantics | `docker/agent-loop.sh`, `worker_config` / schema, UI task editors |
| New Tauri command | `lib.rs` handler list, frontend `invoke`, optionally `capabilities` if permissions change |
| UI navigation / visible headings | `test/playwright/smoke.spec.js` |
| Session / app data files | `AGENTS.md` this table, `USER_GUIDE` if user-facing |

---

## Quick commands

```bash
npm install
npm run tauri dev
npm run build && npm run test:e2e
cargo test --workspace
cargo clippy -p ai_worker_manager --all-targets -- -D warnings
```

---

## Maintainer note

If the architecture changes significantly, **update this file** in the same PR so future agent sessions stay aligned.
