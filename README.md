# Local AI Worker

Desktop app (Tauri + Rust) to manage autonomous AI workers against **Docker-hosted Ollama**, with domain **rules trees** (Git guardrails v1), **SQLite audit** hooks in core, **rolling rate limits**, and **GitHub token** storage via **Windows Credential Manager** / **macOS Keychain**.

[![CI](https://github.com/MichaelJ43/local-ai-worker/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/MichaelJ43/local-ai-worker/actions/workflows/ci.yml)
[![Release](https://github.com/MichaelJ43/local-ai-worker/actions/workflows/release.yml/badge.svg)](https://github.com/MichaelJ43/local-ai-worker/actions/workflows/release.yml)
[![Docs site CI](https://github.com/MichaelJ43/local-ai-worker/actions/workflows/docs-site-ci.yml/badge.svg?branch=main)](https://github.com/MichaelJ43/local-ai-worker/actions/workflows/docs-site-ci.yml)
[![Docs site deploy](https://github.com/MichaelJ43/local-ai-worker/actions/workflows/docs-site-deploy.yml/badge.svg)](https://github.com/MichaelJ43/local-ai-worker/actions/workflows/docs-site-deploy.yml)
[![Docs site soft destroy](https://github.com/MichaelJ43/local-ai-worker/actions/workflows/docs-site-soft-destroy.yml/badge.svg)](https://github.com/MichaelJ43/local-ai-worker/actions/workflows/docs-site-soft-destroy.yml)
[![Docs site auto soft destroy](https://github.com/MichaelJ43/local-ai-worker/actions/workflows/docs-site-auto-soft-destroy.yml/badge.svg)](https://github.com/MichaelJ43/local-ai-worker/actions/workflows/docs-site-auto-soft-destroy.yml)
[![Docs site full destroy](https://github.com/MichaelJ43/local-ai-worker/actions/workflows/docs-site-full-destroy.yml/badge.svg)](https://github.com/MichaelJ43/local-ai-worker/actions/workflows/docs-site-full-destroy.yml)

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
| `docs/` | Documentation, bundled `rules-tree.json` source, [`docs/images/`](docs/images/) screenshots ([`COMPOSE_WORKERS.md`](docs/COMPOSE_WORKERS.md) for optional worker-in-compose) |
| `docker/` | Worker image (`Dockerfile.worker`, agent loop, git/gh wrappers) |
| `src-tauri/resources/compose/` | Bundled Ollama Compose base + GPU override (copied to app data at runtime) |
| `docs-site/` | ASP.NET Core 8 docs + downloads site (Docker image `local-ai-worker-docs`, see [`docs-site/README.md`](docs-site/README.md)) |
| `infra/terraform/` | AWS Terraform for docs Fargate + static (see [`infra/terraform/README.md`](infra/terraform/README.md)) |

## Default model

The **Environment** panel runs host **GPU heuristics** (NVIDIA via `nvidia-smi`, plus macOS / Linux / Windows adapter detection) and sets **`hasDiscreteGpuHint`** in the hardware profile. The suggested tag stays **`gemma4:e2b`** by default; override per worker in the UI / saved JSON.

## Tests

```bash
cargo test --workspace
npm run build
npm run test:e2e
```

See [docs/USER_GUIDE.md](docs/USER_GUIDE.md) for day-to-day usage (workers, Docker, audit), a **[process overview](docs/USER_GUIDE.md#process-overview)** flowchart, **UI screenshots** under [`docs/images/`](docs/images/), and [docs/architecture.md](docs/architecture.md) for the component diagram.

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
| [`.github/workflows/docs-site-ci.yml`](.github/workflows/docs-site-ci.yml) | **Docs site**: .NET unit + Playwright UI tests (Playwright container), Docker build for `docs-site/Dockerfile`. |
| [`.github/workflows/docs-site-deploy.yml`](.github/workflows/docs-site-deploy.yml) | **Push** to `main`/`master` when `docs-site/**`, `docs/images/**`, or these workflows change: build/push **`local-ai-worker-docs`**, Terraform apply Fargate, SSM markers. **Manual** `workflow_dispatch` still supported (optional image tag). Requires AWS OIDC + backend secrets + **Variable** `TF_CUSTOM_DOMAIN`. |
| [`.github/workflows/docs-site-soft-destroy.yml`](.github/workflows/docs-site-soft-destroy.yml) | Static export, Terraform static + S3 sync + CloudFront invalidation; Route53 cutover from ALB → CF is documented in workflow echo (extend with CLI). Needs **`TF_CLOUDFRONT_ACM_CERTIFICATE_ARN`** (us-east-1 ACM). |
| [`.github/workflows/docs-site-auto-soft-destroy.yml`](.github/workflows/docs-site-auto-soft-destroy.yml) | Weekly schedule (disabled until **Variable** `ENABLE_AUTO_SOFT_DESTROY` = `true`): SSM idle gate, then **`gh workflow run`** soft-destroy. |
| [`.github/workflows/docs-site-full-destroy.yml`](.github/workflows/docs-site-full-destroy.yml) | Type `DELETE` to **`terraform destroy`** static then Fargate state (each step may no-op if stack missing). |

### Docs site — GitHub secrets and variables

Configure **OIDC** role (`AWS_ROLE_ARN`), **`AWS_REGION`**, **`TF_ACM_CERTIFICATE_ARN`** (regional ALB cert), **`TF_ROUTE53_HOSTED_ZONE_ID`**, **`TF_STATE_BUCKET`**, **`TF_STATE_LOCK_TABLE`**, **`TF_CLOUDFRONT_ACM_CERTIFICATE_ARN`** (us-east-1, for static/CloudFront). **Repository variable**: **`TF_CUSTOM_DOMAIN`** (e.g. `aiworkers.michaelj43.dev`). Optional: **`ENABLE_AUTO_SOFT_DESTROY`**, **`AUTO_SOFT_DESTROY_IDLE_DAYS`**. Details: [`infra/terraform/README.md`](infra/terraform/README.md) and [`docs-site/README.md`](docs-site/README.md).

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
