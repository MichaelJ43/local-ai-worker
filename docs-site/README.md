# Local AI Worker — docs site

ASP.NET Core 8 Razor site for **https://aiworkers.michaelj43.dev** (and static S3/CloudFront when parked).

## Site map

| Path | Content |
|------|---------|
| `/` | Product overview, capabilities, screenshot grid |
| `/GettingStarted` | Curated first-run steps (install, Ollama, token, first worker) |
| `/Guide` | Renders repo [`docs/USER_GUIDE.md`](../docs/USER_GUIDE.md) (Markdig + Mermaid) |
| `/Architecture` | Renders [`docs/architecture.md`](../docs/architecture.md) |
| `/RepoAgentSandbox` | Renders [`docs/REPO_AGENT_SANDBOX.md`](../docs/REPO_AGENT_SANDBOX.md) |
| `/Download` | Latest installers via GitHub Releases API |

Bundled markdown files are copied next to the published DLL (`docs/*.md`) via `LocalAiWorker.Docs.csproj`. Diagrams use **Mermaid** (`wwwroot/lib/mermaid/mermaid.min.js`, vendored). Changing user-facing docs under `docs/**/*.md` triggers **docs-site CI/deploy** workflows.

## Local run

```bash
cd docs-site/LocalAiWorker.Docs
export M43__StaticAssetsBaseUrl="https://<your-static-assets-origin>"
dotnet run
```

Open http://127.0.0.1:5055 (see `Properties/launchSettings.json`).

## Tests

```bash
cd docs-site
dotnet test LocalAiWorker.Docs.Tests/LocalAiWorker.Docs.Tests.csproj
# UI tests need a running app + Playwright browsers:
dotnet run --project LocalAiWorker.Docs --urls http://127.0.0.1:5055 &
sleep 2
dotnet build LocalAiWorker.Docs.UiTests/LocalAiWorker.Docs.UiTests.csproj
pwsh LocalAiWorker.Docs.UiTests/bin/Debug/net8.0/playwright.ps1 install chromium || true
DOCS_BASE_URL=http://127.0.0.1:5055 dotnet test LocalAiWorker.Docs.UiTests/LocalAiWorker.Docs.UiTests.csproj
```

CI runs the app in the background and installs Playwright automatically.

## Docker

From repo root:

```bash
docker build -f docs-site/Dockerfile -t local-ai-worker-docs:local .
docker run --rm -p 8080:8080 -e M43__StaticAssetsBaseUrl="https://static.michaelj43.dev" local-ai-worker-docs:local
```

## Static export (Soft-Destroy)

With the published app listening on `DOCS_URL`, [`scripts/static-export.sh`](scripts/static-export.sh) snapshots `/`, `/Download`, `/GettingStarted`, `/Guide`, `/Architecture`, and `/RepoAgentSandbox` plus `docs/images` into `OUT_DIR`.

```bash
chmod +x docs-site/scripts/static-export.sh
DOCS_URL=http://127.0.0.1:5055 OUT_DIR=./dist-static docs-site/scripts/static-export.sh
```

## Terraform

See [`infra/terraform/docs-site-fargate/`](../infra/terraform/docs-site-fargate/) and [`infra/terraform/docs-site-static/`](../infra/terraform/docs-site-static/).
