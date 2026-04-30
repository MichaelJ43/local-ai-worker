# Repo autonomous worker — sandbox notes (experimental)

This captures how **Docker networking** and **`repoSandboxPolicy`** interact with the repo-agent loop (`AI_REPO_AGENT=1`).

## Repo tree mutations

Configured in **`repoSandboxPolicy`** (Advanced → JSON) and materialized into **`agent-config.json` → `sandboxPolicy`**.

| Flag | Meaning |
|------|--------|
| **`fileWritesEnabled`** | If true (and tier is **`apply_git`** / **`apply_github`**), **`fileWrites[]`** objects in structured model JSON can create/delete files **under `/workspace/repo`**. Paths reject `..`, absolute paths, doubled slashes; **`.git/*`** blocked unless **`allowGitInternalsWrites`**. Sizes capped by **`patchMaxFileBytes`** / **`patchMaxTotalBytes`**. |
| **`applyUnifiedDiffsEnabled`** | If true, each string in model **`unifiedDiffs[]`** is passed to **`git apply`** (via real **`git`**, not the guard wrapper—same as scripted read-only observations). **`patchMaxUnifiedDiffBytes`** caps each diff. |

**`observe`** never applies writes or patches; `commands[]`/`fileWrites`/diffs only record reflections.

Always rebuild the worker image after script changes:

`docker build -f docker/Dockerfile.worker -t local-ai-worker-agent:latest .`

## Docker `--network`

**`dockerNetwork`** on the worker is passed to **`docker run --network …`**.

- **`bridge`** (or empty): daemon default IPv4 NAT; container can usually reach the public internet, **GitHub**, and **`host.docker.internal`** (Ollama).
- **`none`**: **no** network interfaces except loopback—**`git fetch` / `gh` / Ollama over `host.docker.internal` will fail**. Only useful for dry runs that never talk to remotes.
- **`docker network create --internal …`**: **no default route to the internet** on Linux; you generally **cannot** reach **github.com** or **ghcr.io** unless you add a **proxy** or **egress allowlist** outside this app. The app does **not** auto-configure HTTP proxies or iptables.

**macOS Docker Desktop** behavior vs **Linux** differs; treat network isolation as **operator-defined** by the network you attach.

### Practical patterns

1. **Default (bridge)** + rely on **GitHub token scope**, **repo allowlists** in guardrails, and **dummy repos** for experiments.
2. **Custom user-defined network** shared with a **controlled HTTP/HTTPS proxy** container; point **`HTTP_PROXY`/`HTTPS_PROXY`** via worker env (future: first-class fields).
3. **Fully internal network** only if you accept **no direct GitHub** (e.g. host-side sync only—which this design does not implement today).

For productized egress rules, expect follow-up work (proxy env, sidecar, or Linux-only **iptables** + `NET_ADMIN`—not enabled by default).
