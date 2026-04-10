# Rules tree (`rules-tree.json`)

The app bundles a default tree from this folder at compile time (`ai_worker_core::DEFAULT_RULES_TREE_JSON`). To customize for your org, edit `rules-tree.json` here and rebuild, or replace the file and load from disk in a future version.

## Domains

- **`git`**: Full guardrails for Git/GitHub maintenance (rate limits, branch safety, merge policy, audit flags). See the implementation plan for the authoritative key list.
- **`market`**: Stub (`enabled: false`) for future domains.

## Per-worker overrides

Worker definitions may include a `guardrailOverrides` object that is **deep-merged** into the domain’s `guardrails` JSON before prompts are assembled and before enforcement code runs.

## Rolling windows

- **Per day** limits use a **rolling 24-hour** window.
- **Per hour** limits use a **rolling 60-minute** window.

## Prompt assembly

The desktop app resolves the active domain, merges overrides, and builds a system-style section listing guardrails JSON plus numbered `promptGuidelines`.
