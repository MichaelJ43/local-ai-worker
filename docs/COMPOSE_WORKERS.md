# Compose topology: Ollama + optional worker services

The desktop app runs **Ollama** from bundled Compose (`ollama-compose.base.yml` + optional GPU file) and starts **workers** with `docker run` (one container per worker). That keeps worker definitions dynamic without regenerating Compose on every UI change.

## Ollama stack

- **Bundled** (app): materialized under your app data directory, project name `local-ai-ollama`.
- **Repo root** [`docker-compose.yml`](../docker-compose.yml): same shape for manual `docker compose up`.
- The Ollama **image tag is pinned** (see YAML) for reproducible installs; bump the tag when you intentionally upgrade Ollama. Model tags (e.g. `gemma4:e2b`) are still pulled **inside** the container with `ollama pull`.

## Workers in Compose (optional)

If you prefer a single `docker compose` graph (e.g. for homelab GitOps), add a service that reuses the same image the app uses:

```yaml
# fragment — merge with your compose file or use `docker compose -f ... -f ...`
services:
  worker-example:
    image: local-ai-worker-agent:latest
    restart: unless-stopped
    volumes:
      - ./contexts/example/context.json:/workspace/context.json:rw
      - worker_lt_example:/persist
      - ./contexts/example/guardrails.effective.json:/workspace/guardrails.effective.json:ro
      - ./contexts/example/system-prompt.txt:/workspace/system-prompt.txt:ro
      - ./contexts/example/agent-config.json:/workspace/agent-config.json:ro
      - ./audit.sqlite3:/persist/audit.sqlite3:rw
    environment:
      AI_WORKER_ID: example
      OLLAMA_HOST: http://ollama:11434
      AI_GUARD_STATE_DIR: /persist
      AI_AUDIT_DB: /persist/audit.sqlite3
      AI_AGENT_LOOP: "1"
    depends_on:
      - ollama
```

Adjust host paths, join the app’s Docker network (or use `host.docker.internal` on Docker Desktop), and rebuild the agent image from the repo root:

```bash
docker build -f docker/Dockerfile.worker -t local-ai-worker-agent:latest .
```

The app already materializes `guardrails.effective.json`, `system-prompt.txt`, and `agent-config.json` under `%LOCALAPPDATA%\\local-ai-worker\\workers\\<id>\\` (Windows) or the platform equivalent—copy or bind-mount those files if you manage workers outside the UI.

## Disabling the in-container agent loop

The worker image runs [`docker/agent-loop.sh`](../docker/agent-loop.sh) in the background (Ollama chat + context updates). To get a shell-only container, set:

`AI_AGENT_LOOP=0`

and override the command if needed (or use an older image that only ran `sleep infinity`).
