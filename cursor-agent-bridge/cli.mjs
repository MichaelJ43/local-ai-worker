#!/usr/bin/env node
/**
 * Reads JSON from stdin, runs @cursor/sdk (local or cloud), prints one JSON line on stdout.
 * Environment: CURSOR_API_KEY (required unless --validate-only).
 */

import fs from "node:fs";
import process from "node:process";

async function main(argv) {
  const validateOnly = argv.includes("--validate-only");
  const dryRun = argv.includes("--dry-run");

  const raw = fs.readFileSync(0, "utf8");
  const payload = JSON.parse(raw);

  if (validateOnly) {
    if (!payload.promptText || typeof payload.promptText !== "string") {
      console.error("validate-only: promptText required");
      process.exit(1);
    }
    console.log(JSON.stringify({ ok: true, validateOnly: true }));
    return;
  }

  if (dryRun) {
    console.log(
      JSON.stringify({
        ok: true,
        dryRun: true,
        mode: payload.mode,
        wouldUseLocal: !!(payload.local?.cwd),
      }),
    );
    return;
  }

  const apiKey = process.env.CURSOR_API_KEY;
  if (!apiKey) {
    console.error("CURSOR_API_KEY is not set");
    process.exit(1);
  }

  const { Agent } = await import("@cursor/sdk");

  const model = payload.modelId ? { id: String(payload.modelId) } : undefined;
  const promptText = String(payload.promptText || "");

  try {
    if (payload.mode === "cloud") {
      const repos = payload.cloud?.repos;
      if (!Array.isArray(repos) || repos.length === 0) {
        throw new Error('mode "cloud" requires cloud.repos array');
      }

      const agent = await Agent.create({
        apiKey,
        model,
        cloud: {
          repos: repos.map((r) => ({
            url: String(r.url),
            startingRef: r.startingRef != null ? String(r.startingRef) : "main",
          })),
          autoCreatePR: !!payload.cloud?.autoCreatePr,
        },
      });

      const run = await agent.send(promptText);
      const rr = await run.wait();

      await disposeAsync(agent);

      console.log(
        JSON.stringify({
          ok: true,
          mode: "cloud",
          agentId: agent.agentId,
          runId: rr.id,
          assistantTextPreview: (rr.result || "").slice(0, 8000),
        }),
      );
      return;
    }

    const cwd = payload.local?.cwd;
    if (!cwd || typeof cwd !== "string") {
      throw new Error('mode "local" requires local.cwd');
    }

    const rr = await Agent.prompt(promptText, {
      apiKey,
      model,
      local: { cwd },
    });

    console.log(
      JSON.stringify({
        ok: true,
        mode: "local",
        runId: rr.id,
        assistantTextPreview: (rr.result || "").slice(0, 8000),
      }),
    );
  } catch (e) {
    const err = /** @type {{ message?: string }} */ (e || {});
    const msg = err.message || String(e);
    process.stderr.write(`${msg}\n`);
    console.log(JSON.stringify({ ok: false, error: msg }));
    process.exit(1);
  }
}

/**
 * @param {unknown} o
 */
async function disposeAsync(o) {
  if (o && typeof o === "object" && Symbol.asyncDispose in o) {
    await /** @type {{ [Symbol.asyncDispose]: () => Promise<void> }} */ (o)[
      Symbol.asyncDispose
    ]();
  } else if (o && typeof o === "object" && "close" in o) {
    /** @type {{ close: () => void }} */ (o).close();
  }
}

main(process.argv.slice(2)).catch((e) => {
  process.stderr.write(String(e?.stack || e));
  process.exit(1);
});
