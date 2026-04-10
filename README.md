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

With **no GPU** and typical **16 GB RAM**, the app suggests **`gemma4:e2b`**. Override per worker in the UI / saved JSON.

## Tests

```bash
cargo test --workspace
npm run build
npm run test:e2e
```

See [docs/USER_GUIDE.md](docs/USER_GUIDE.md) for day-to-day usage (workers, Docker, audit).

## Release build

```bash
npm run build
npm run tauri build
```

Enable bundling in `src-tauri/tauri.conf.json` (`bundle.active`) when you are ready for installers; icons live under `src-tauri/icons/`.

## License

MIT
