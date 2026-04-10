import { invoke } from "@tauri-apps/api/core";

async function refreshEnv() {
  const el = document.getElementById("env-status");
  try {
    const docker = await invoke("docker_status");
    const hw = await invoke("hardware_profile");
    el.textContent = `Docker: ${docker.available ? "OK" : "unavailable"}${
      docker.version ? ` (server ${docker.version})` : ""
    }
Suggested model: ${hw.suggestedModel}
RAM: ${(hw.totalMemoryBytes / 1024 ** 3).toFixed(1)} GiB | CPUs: ${hw.cpuCount}
${hw.notes}`;
  } catch (e) {
    el.textContent = String(e);
  }
  await refreshComposeHint();
}

async function refreshComposeHint() {
  const el = document.getElementById("compose-hint");
  try {
    const h = await invoke("ollama_stack_gpu_hint");
    el.textContent = `Compose dir: ${h.composeDir}
nvidia-smi: ${h.nvidiaSmiAvailable ? "yes" : "no"} | Auto GPU: ${h.autoUseGpu ? "on" : "off"}`;
  } catch (e) {
    el.textContent = String(e);
  }
}

function gpuModeArg() {
  const v = document.getElementById("select-gpu-mode").value;
  if (v === "auto") return null;
  if (v === "on") return true;
  return false;
}

async function refreshTokenStatus() {
  const el = document.getElementById("token-status");
  try {
    const ok = await invoke("github_token_configured");
    el.textContent = ok ? "A token is stored." : "No token stored.";
  } catch (e) {
    el.textContent = String(e);
  }
}

async function refreshAppLog() {
  const el = document.getElementById("app-log");
  try {
    const lines = await invoke("app_log_lines");
    el.textContent = lines.length ? lines.join("\n") : "(empty)";
  } catch (e) {
    el.textContent = String(e);
  }
}

async function refreshAudit() {
  const el = document.getElementById("audit-out");
  try {
    const rows = await invoke("audit_recent_github", { limit: 30 });
    el.textContent = rows.length ? JSON.stringify(rows, null, 2) : "(no rows)";
  } catch (e) {
    el.textContent = String(e);
  }
}

function workerTemplate(id) {
  return {
    id,
    name: "Worker",
    maintenanceDomain: "git",
    modelOverride: null,
    ollamaHost: null,
    enabled: true,
    tasks: [
      {
        id: crypto.randomUUID(),
        title: "Maintenance pass",
        schedule: { kind: "cadence", intervalSeconds: 3600 },
      },
    ],
    guardrailOverrides: null,
    contextPath: null,
    longTermVolume: null,
    dockerImage: null,
  };
}

function escapeAttr(s) {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/"/g, "&quot;")
    .replace(/</g, "&lt;");
}

function escapeTextarea(s) {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;");
}

function findWorkerIndex(wid) {
  return workersCache.findIndex((w) => w.id === wid);
}

function renderWorkers(workers) {
  const root = document.getElementById("workers-list");
  root.innerHTML = "";
  workers.forEach((w) => {
    const gr =
      w.guardrailOverrides != null
        ? JSON.stringify(w.guardrailOverrides, null, 2)
        : "";
    const tasksHtml = w.tasks
      .map(
        (t, ti) =>
          `<div class="task-row">${escapeAttr(t.title)} <span class="task-kind">(${t.schedule.kind})</span> <button type="button" class="linkish" data-wid="${escapeAttr(w.id)}" data-task-idx="${ti}" data-action="task-remove">remove</button></div>`,
      )
      .join("");

    const card = document.createElement("div");
    card.className = "worker-card";
    card.dataset.workerId = w.id;
    card.innerHTML = `
      <div class="worker-head">
        <strong>${escapeAttr(w.name)}</strong>
        <code class="wid">${escapeAttr(w.id)}</code>
      </div>
      <label>Name <input data-k="name" data-wid="${escapeAttr(w.id)}" type="text" value="${escapeAttr(w.name)}" /></label>
      <label>Domain <input data-k="maintenanceDomain" data-wid="${escapeAttr(w.id)}" type="text" value="${escapeAttr(w.maintenanceDomain)}" /></label>
      <label>Model override <input data-k="modelOverride" data-wid="${escapeAttr(w.id)}" type="text" value="${escapeAttr(w.modelOverride || "")}" placeholder="gemma4:e2b" /></label>
      <label>Docker image <input data-k="dockerImage" data-wid="${escapeAttr(w.id)}" type="text" value="${escapeAttr(w.dockerImage || "")}" placeholder="local-ai-worker-agent:latest" /></label>
      <label><input data-k="enabled" data-wid="${escapeAttr(w.id)}" type="checkbox" ${w.enabled ? "checked" : ""} /> Enabled</label>
      <label>Guardrail overrides (JSON object, merged into domain guardrails)
        <textarea data-field="guardrails" data-wid="${escapeAttr(w.id)}" rows="5" class="code">${escapeTextarea(gr)}</textarea>
      </label>
      <div class="task-block">
        <div class="task-label">Tasks</div>
        ${tasksHtml || "<p class=\"hint\">No tasks</p>"}
        <div class="row">
          <button type="button" data-action="task-cadence" data-wid="${escapeAttr(w.id)}">+ Cadence</button>
          <button type="button" data-action="task-oneshot" data-wid="${escapeAttr(w.id)}">+ One-shot</button>
        </div>
      </div>
      <div class="paths hint">
        Context: ${w.contextPath ? escapeAttr(w.contextPath) : "— (run Prepare)"}<br/>
        Volume: ${w.longTermVolume ? escapeAttr(w.longTermVolume) : "—"}
      </div>
      <div class="row wrap">
        <button type="button" data-docker="prepare" data-wid="${escapeAttr(w.id)}">Prepare storage</button>
        <button type="button" data-docker="start" data-wid="${escapeAttr(w.id)}">Start container</button>
        <button type="button" data-docker="stop" data-wid="${escapeAttr(w.id)}" class="secondary">Stop</button>
        <button type="button" data-docker="recreate" data-wid="${escapeAttr(w.id)}" class="secondary">Recreate</button>
        <button type="button" data-docker="status" data-wid="${escapeAttr(w.id)}" class="secondary">Status</button>
        <button type="button" data-docker="logs" data-wid="${escapeAttr(w.id)}" class="secondary">Logs</button>
      </div>
      <pre class="worker-docker-out preview small" data-wid="${escapeAttr(w.id)}"></pre>
    `;
    root.appendChild(card);
  });

  root.querySelectorAll("input[data-k]").forEach((inp) => {
    inp.addEventListener("change", () => {
      const wid = inp.dataset.wid;
      const i = findWorkerIndex(wid);
      if (i < 0) return;
      const k = inp.dataset.k;
      if (k === "enabled") workersCache[i].enabled = inp.checked;
      else if (k === "modelOverride")
        workersCache[i].modelOverride = inp.value.trim() || null;
      else if (k === "dockerImage")
        workersCache[i].dockerImage = inp.value.trim() || null;
      else workersCache[i][k] = inp.value;
    });
  });

  root.querySelectorAll("textarea[data-field='guardrails']").forEach((ta) => {
    ta.addEventListener("change", () => {
      const wid = ta.dataset.wid;
      const i = findWorkerIndex(wid);
      if (i < 0) return;
      const raw = ta.value.trim();
      if (!raw) {
        workersCache[i].guardrailOverrides = null;
        return;
      }
      try {
        workersCache[i].guardrailOverrides = JSON.parse(raw);
      } catch {
        document.getElementById("workers-msg").textContent =
          "Invalid JSON in guardrail overrides.";
      }
    });
  });
}

let workersCache = [];

async function loadWorkers() {
  workersCache = await invoke("get_workers");
  renderWorkers(workersCache);
}

document.getElementById("workers-list").addEventListener("click", async (e) => {
  const t = e.target;
  const rm = t.closest("[data-action='task-remove']");
  if (rm) {
    const wid = rm.dataset.wid;
    const ti = Number(rm.dataset.taskIdx);
    const i = findWorkerIndex(wid);
    if (i >= 0 && ti >= 0) {
      workersCache[i].tasks.splice(ti, 1);
      renderWorkers(workersCache);
    }
    return;
  }
  const cad = t.closest("[data-action='task-cadence']");
  if (cad) {
    const wid = cad.dataset.wid;
    const i = findWorkerIndex(wid);
    if (i >= 0) {
      workersCache[i].tasks.push({
        id: crypto.randomUUID(),
        title: "Cadence task",
        schedule: { kind: "cadence", intervalSeconds: 3600 },
      });
      renderWorkers(workersCache);
    }
    return;
  }
  const one = t.closest("[data-action='task-oneshot']");
  if (one) {
    const wid = one.dataset.wid;
    const i = findWorkerIndex(wid);
    if (i >= 0) {
      workersCache[i].tasks.push({
        id: crypto.randomUUID(),
        title: "One-shot task",
        schedule: { kind: "oneShot" },
      });
      renderWorkers(workersCache);
    }
    return;
  }

  const btn = t.closest("[data-docker]");
  if (!btn) return;
  const wid = btn.dataset.wid;
  const action = btn.dataset.docker;
  const card = btn.closest(".worker-card");
  const outEl = card ? card.querySelector("pre.worker-docker-out") : null;
  const setOut = (s) => {
    if (outEl) outEl.textContent = s;
  };
  try {
    if (action === "prepare") {
      const info = await invoke("worker_storage_prepare", { workerId: wid });
      setOut(JSON.stringify(info, null, 2));
      await loadWorkers();
    } else if (action === "start") {
      setOut(await invoke("worker_docker_start", { workerId: wid }));
    } else if (action === "stop") {
      setOut(await invoke("worker_docker_stop", { workerId: wid }));
    } else if (action === "recreate") {
      setOut(await invoke("worker_docker_recreate", { workerId: wid }));
    } else if (action === "status") {
      setOut(await invoke("worker_docker_status", { workerId: wid }));
    } else if (action === "logs") {
      setOut(await invoke("worker_docker_logs", { workerId: wid, tail: 120 }));
    }
    await refreshAppLog();
  } catch (err) {
    setOut(String(err));
  }
});

document.getElementById("btn-refresh-env").addEventListener("click", refreshEnv);

document.getElementById("btn-compose-up").addEventListener("click", async () => {
  const out = document.getElementById("compose-out");
  out.textContent = "Running…";
  try {
    const text = await invoke("ollama_stack_up", { useGpu: gpuModeArg() });
    out.textContent = text || "OK.";
    await refreshComposeHint();
    await refreshEnv();
    await refreshAppLog();
  } catch (e) {
    out.textContent = String(e);
  }
});
document.getElementById("btn-compose-down").addEventListener("click", async () => {
  const out = document.getElementById("compose-out");
  out.textContent = "Running…";
  try {
    const text = await invoke("ollama_stack_down");
    out.textContent = text || "OK.";
    await refreshEnv();
    await refreshAppLog();
  } catch (e) {
    out.textContent = String(e);
  }
});
document.getElementById("btn-compose-ps").addEventListener("click", async () => {
  const out = document.getElementById("compose-out");
  try {
    out.textContent = await invoke("ollama_stack_status");
  } catch (e) {
    out.textContent = String(e);
  }
});

document.getElementById("btn-save-token").addEventListener("click", async () => {
  const v = document.getElementById("input-token").value.trim();
  if (!v) return;
  try {
    await invoke("set_github_token", { token: v });
    document.getElementById("input-token").value = "";
    await refreshTokenStatus();
  } catch (e) {
    document.getElementById("token-status").textContent = String(e);
  }
});
document.getElementById("btn-clear-token").addEventListener("click", async () => {
  try {
    await invoke("delete_github_token");
    await refreshTokenStatus();
  } catch (e) {
    document.getElementById("token-status").textContent = String(e);
  }
});

document.getElementById("btn-add-worker").addEventListener("click", () => {
  workersCache.push(workerTemplate(crypto.randomUUID()));
  renderWorkers(workersCache);
});

document.getElementById("btn-save-workers").addEventListener("click", async () => {
  const msg = document.getElementById("workers-msg");
  try {
    await invoke("save_workers", { workers: workersCache });
    msg.textContent = "Saved.";
    await loadWorkers();
  } catch (e) {
    msg.textContent = String(e);
  }
});

document.getElementById("btn-preview-prompt").addEventListener("click", async () => {
  const domain = document.getElementById("input-domain").value.trim() || "git";
  const pre = document.getElementById("prompt-preview");
  try {
    let guardrailOverrides = null;
    const raw = document.getElementById("prompt-guardrails-override").value.trim();
    if (raw) guardrailOverrides = JSON.parse(raw);
    const text = await invoke("assemble_prompt_preview", {
      domain,
      guardrailOverrides,
      contextExcerpt: null,
    });
    pre.textContent = text;
  } catch (e) {
    pre.textContent = String(e);
  }
});

document.getElementById("btn-list-models").addEventListener("click", async () => {
  const host = document.getElementById("input-ollama").value.trim() || null;
  const pre = document.getElementById("ollama-out");
  try {
    const tags = await invoke("ollama_list_models", { host });
    pre.textContent = tags.join("\n");
  } catch (e) {
    pre.textContent = String(e);
  }
});

document.getElementById("btn-audit-refresh").addEventListener("click", refreshAudit);
document.getElementById("btn-app-log-refresh").addEventListener("click", refreshAppLog);

refreshEnv();
refreshComposeHint().catch(() => {});
refreshTokenStatus();
refreshAppLog().catch(() => {});
refreshAudit().catch(() => {});
loadWorkers().catch((e) => {
  document.getElementById("workers-msg").textContent = String(e);
});
