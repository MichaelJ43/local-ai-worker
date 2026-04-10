# Local AI Worker

Desktop app (Tauri + Rust) to manage autonomous AI workers against **Docker-hosted Ollama**, with domain **rules trees** (Git guardrails v1), **SQLite audit** hooks in core, **rolling rate limits**, and **GitHub token** storage via **Windows Credential Manager** / **macOS Keychain**.

## Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) 18+ (for Vite + Tauri CLI via npm)
- [Docker Desktop](https://www.docker.com/products/docker-desktop/) (for Ollama and worker sandboxes)

## Quick start

1. **Ollama via the app (recommended)**  
   Open the app and use **Ollama stack → Compose up**. The app writes bundled YAML to your app data directory (see the UI for the path) and runs `docker compose` with optional **GPU** merge when `nvidia-smi` is available (or you can force CPU/GPU in the dropdown).

   Then pull a model (once the container is up):

   ```bash
   docker exec -it local-ai-ollama ollama pull gemma4:e2b
   ```

   **Or** use the repo root compose file manually:

   ```bash
   docker compose up -d
   docker exec -it local-ai-ollama ollama pull gemma4:e2b
   ```

2. Install JS deps and run the app in dev:

   ```bash
   npm install
   npm run tauri dev
   ```

3. In the UI, save your GitHub token (stored in the OS secure store), configure workers, and use **Prompt preview** to verify the **git** rules pack.

## Project layout

| Path | Purpose |
|------|---------|
| `src/` | Vite frontend (HTML/CSS/JS) |
| `src-tauri/` | Tauri host + commands |
| `crates/ai_worker_core/` | Shared Rust: rules, context, audit, Ollama client, hardware, rate limits, `worker-guard` |
| `test/` | Integration tests (Rust) |
| `docs/` | Documentation and bundled `rules-tree.json` source ([`COMPOSE_WORKERS.md`](docs/COMPOSE_WORKERS.md) for optional worker-in-compose) |
| `docker/` | Worker image (`Dockerfile.worker`, agent loop, git/gh wrappers) |
| `src-tauri/resources/compose/` | Bundled Ollama Compose base + GPU override (copied to app data at runtime) |

## Default model

The **Environment** panel runs host **GPU heuristics** (NVIDIA via `nvidia-smi`, plus macOS / Linux / Windows adapter detection) and sets **`hasDiscreteGpuHint`** in the hardware profile. The suggested tag stays **`gemma4:e2b`** by default; override per worker in the UI / saved JSON.

## Tests

```bash
cargo test --workspace
npm run build
npm run test:e2e
```

See [docs/USER_GUIDE.md](docs/USER_GUIDE.md) for day-to-day usage (workers, Docker, audit).

## Release build (local)

```bash
npm run build
npm run tauri build
```

Release-style build (injects `GITHUB_REPOSITORY` into the updater endpoint when set):

```bash
set GITHUB_REPOSITORY=MichaelJ43/local-ai-worker
npm run tauri:build
```

Icons live under `src-tauri/icons/`. Updates are **signed** with a minisign key pair: the **public** key is in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`); never commit the **private** key (see `.gitignore`).

## Versioning, CI, and GitHub Releases

This repo follows the same pattern as [Echo](https://github.com/MichaelJ43/echo):

| Piece | Purpose |
|-------|---------|
| [`.github/workflows/ci.yml`](.github/workflows/ci.yml) | Path-filtered CI: frontend build, Playwright, Rust `test` + `clippy` on Ubuntu, Windows, and macOS. |
| [`.github/workflows/version-bump.yml`](.github/workflows/version-bump.yml) | On **merged PR to `main`**, bumps **patch** by default (or use `+(semver:minor)` / `+(semver:major)` in the PR title), commits, tags `v*`, pushes with **`RELEASE_PUSH_TOKEN`**, then dispatches **Release**. Manual **workflow_dispatch** can choose patch/minor/major. |
| [`.github/workflows/release.yml`](.github/workflows/release.yml) | **workflow_dispatch** with a tag: builds Linux / Windows / macOS via **`tauri-apps/tauri-action`**, uploads installers and **`latest.json`** to the GitHub Release (for the in-app updater). |

### Required repository secrets

1. **`RELEASE_PUSH_TOKEN`** — Fine-grained or classic PAT with **Contents: Read and write** on this repo. Pushes the version-bump commit and tag (GitHub’s default `GITHUB_TOKEN` does not trigger downstream workflows from automation). You already added this.
2. **`TAURI_SIGNING_PRIVATE_KEY`** — Full text of the **private** minisign key used to sign update artifacts. Must match the **public** key in `tauri.conf.json`. Generate locally with:

   ```bash
   cd src-tauri
   npx @tauri-apps/cli signer generate -w .updater-signing.key
   ```

   (Use `-p` / `--ci` per CLI help if you want non-interactive.) Paste the **private** file contents into the secret; do **not** commit `.updater-signing.key`. If you generate a **new** key pair, update **`pubkey`** in `tauri.conf.json` in the same commit as rotating the secret.

### Auto-updates

The desktop app checks for updates **on launch and hourly**. Installers and `latest.json` are published on [Releases](https://github.com/MichaelJ43/local-ai-worker/releases). After the first signed release is published, installs built with the matching public key can update in-app.

## License

MIT
