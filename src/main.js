import { invoke } from "@tauri-apps/api/core";
import {
  fetchUpdateIfAvailable,
  openReleasesPage,
  startUpdateCheckScheduler,
} from "./updater.js";

const VIEW_TITLES = {
  overview: "Overview",
  stack: "Ollama stack",
  workers: "Workers",
  secrets: "Secrets",
  diagnostics: "Diagnostics",
};

/** @type {{ key: string, label: string, selectable: boolean }[]} */
let domainsCache = [];
const draftWorkerIds = new Set();

function showView(navId) {
  document.querySelectorAll(".nav-item").forEach((b) => {
    b.classList.toggle("active", b.dataset.nav === navId);
  });
  document.querySelectorAll(".view").forEach((v) => {
    v.classList.toggle("active", v.dataset.view === navId);
  });
  const title = document.getElementById("view-title");
  if (title) title.textContent = VIEW_TITLES[navId] || navId;
}

document.querySelectorAll(".nav-item").forEach((btn) => {
  btn.addEventListener("click", () => showView(btn.dataset.nav));
});

function openModal(el) {
  if (el) el.hidden = false;
}

function closeModal(el) {
  if (el) el.hidden = true;
}

function wireModalOverlay(id) {
  const el = document.getElementById(id);
  if (!el) return;
  el.addEventListener("click", (e) => {
    if (e.target === el) closeModal(el);
  });
  el.querySelectorAll("[data-close-modal]").forEach((btn) => {
    btn.addEventListener("click", () => closeModal(el));
  });
}

wireModalOverlay("modal-domain");
wireModalOverlay("modal-tasks");

async function loadRulesDomains() {
  try {
    domainsCache = await invoke("rules_domains_list");
  } catch {
    domainsCache = [{ key: "git", label: "Git / GitHub maintenance", selectable: true }];
  }
}

async function maybeShowRestorePrompt() {
  try {
    const pending = await invoke("session_peek_pending_restore");
    if (!pending) return;
    const lead = document.getElementById("modal-restore-lead");
    const snap = pending.enabledByWorkerId || {};
    const n = Object.keys(snap).length;
    lead.textContent = `When you last closed the app, at least one worker was marked enabled in your saved config. That snapshot had ${n} worker id(s). Choose how to set the enabled checkboxes for this session:`;
    openModal(document.getElementById("modal-restore"));
  } catch (e) {
    console.warn(e);
  }
}

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
  if (!el) return;
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
    envFromSecrets: [],
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

function ensureEnvBindings(w) {
  if (!Array.isArray(w.envFromSecrets)) w.envFromSecrets = [];
}

function domainSelectHtml(w) {
  const inList = domainsCache.some((d) => d.key === w.maintenanceDomain);
  const otherSel = !inList;
  const opts = domainsCache
    .map(
      (d) =>
        `<option value="${escapeAttr(d.key)}" ${!d.selectable ? "disabled" : ""} ${w.maintenanceDomain === d.key ? "selected" : ""}>${escapeAttr(d.label)} (${escapeAttr(d.key)})</option>`,
    )
    .join("");
  const otherClass = inList ? "domain-other hidden" : "domain-other";
  const customVal = inList ? "" : escapeAttr(w.maintenanceDomain);
  return `
    <label>Domain
      <select data-domain-select data-wid="${escapeAttr(w.id)}">
        ${opts}
        <option value="__other__" ${otherSel ? "selected" : ""}>Other (custom key)…</option>
      </select>
    </label>
    <label class="${otherClass}" data-domain-other-wrap="${escapeAttr(w.id)}">
      <span>Custom domain key</span>
      <input type="text" data-domain-custom data-wid="${escapeAttr(w.id)}" value="${customVal}" placeholder="e.g. git" />
    </label>
    <p class="hint">The domain chooses which rule pack and prompt guidelines apply.</p>
    <div class="row tight">
      <button type="button" class="secondary small-pad" data-action="show-domain-help">What is a domain?</button>
      <button type="button" class="secondary small-pad" data-action="show-tasks-help">Tasks &amp; cadence</button>
    </div>
  `;
}

async function refreshSecretDatalist() {
  const dl = document.getElementById("secret-keys-datalist");
  if (!dl) return;
  try {
    const keys = await invoke("secret_keys_list");
    dl.innerHTML = keys.map((k) => `<option value="${escapeAttr(k)}"></option>`).join("");
  } catch {
    dl.innerHTML = "";
  }
}

async function refreshSecretsTable() {
  const tbody = document.getElementById("secrets-tbody");
  const msg = document.getElementById("secrets-msg");
  if (!tbody) return;
  try {
    const keys = await invoke("secret_keys_list");
    if (keys.length === 0) {
      tbody.innerHTML = `<tr><td colspan="2" class="hint">No secrets yet.</td></tr>`;
    } else {
      tbody.innerHTML = keys
        .map(
          (k) =>
            `<tr><td><code>${escapeAttr(k)}</code></td><td><button type="button" class="secondary small-pad" data-secret-remove="${escapeAttr(k)}">Remove</button></td></tr>`,
        )
        .join("");
    }
    if (msg) msg.textContent = "";
  } catch (e) {
    tbody.innerHTML = "";
    if (msg) msg.textContent = String(e);
  }
  await refreshSecretDatalist();
}

function renderWorkers(workers) {
  const root = document.getElementById("workers-list");
  root.innerHTML = "";
  workers.forEach((w) => {
    ensureEnvBindings(w);
    const gr =
      w.guardrailOverrides != null
        ? JSON.stringify(w.guardrailOverrides, null, 2)
        : "";
    const tasksHtml = w.tasks
      .map((t, ti) => {
        const isCad = t.schedule.kind === "cadence";
        const intv = isCad ? t.schedule.intervalSeconds : 3600;
        return `<div class="task-row-editable" data-wid="${escapeAttr(w.id)}" data-task-idx="${ti}">
  <input type="text" data-task-field="title" data-wid="${escapeAttr(w.id)}" data-task-idx="${ti}" value="${escapeAttr(t.title)}" />
  <select data-task-field="kind" data-wid="${escapeAttr(w.id)}" data-task-idx="${ti}">
    <option value="cadence" ${isCad ? "selected" : ""}>Repeating (cadence)</option>
    <option value="oneShot" ${!isCad ? "selected" : ""}>One-shot</option>
  </select>
  <span class="task-interval-wrap ${isCad ? "" : "hidden"}" data-task-interval-wrap>
    <label class="inline tight"><span>Interval (sec)</span>
      <input type="number" min="30" step="1" data-task-field="interval" data-wid="${escapeAttr(w.id)}" data-task-idx="${ti}" value="${intv}" />
    </label>
  </span>
  <button type="button" class="linkish" data-wid="${escapeAttr(w.id)}" data-task-idx="${ti}" data-action="task-remove">remove</button>
</div>`;
      })
      .join("");

    const bindHtml = w.envFromSecrets
      .map(
        (b, bi) =>
          `<div class="env-bind-row">
 <input type="text" data-env-bind="1" data-wid="${escapeAttr(w.id)}" data-env-idx="${bi}" data-env-part="envVar" value="${escapeAttr(b.envVar || "")}" placeholder="GITHUB_TOKEN" />
            <span class="env-bind-arrow">←</span>
            <input type="text" data-env-bind="1" data-wid="${escapeAttr(w.id)}" data-env-idx="${bi}" data-env-part="secretKey" value="${escapeAttr(b.secretKey || "")}" placeholder="secret key" list="secret-keys-datalist" />
            <button type="button" class="secondary small-pad" data-action="env-bind-remove" data-wid="${escapeAttr(w.id)}" data-env-idx="${bi}">Remove</button>
          </div>`,
      )
      .join("");

    const isDraft = draftWorkerIds.has(w.id);
    const card = document.createElement("div");
    card.className = `worker-card ${w.enabled ? "worker-card--on" : "worker-card--off"}`;
    card.dataset.workerId = w.id;
    card.innerHTML = `
      <div class="worker-head">
        <div>
          <strong>${escapeAttr(w.name)}</strong>
          <code class="wid">${escapeAttr(w.id)}</code>
        </div>
        <span class="worker-status ${w.enabled ? "worker-status--on" : "worker-status--off"}">${w.enabled ? "Enabled" : "Disabled"}</span>
      </div>
      ${isDraft ? `<div class="row"><button type="button" class="secondary" data-action="worker-discard" data-wid="${escapeAttr(w.id)}">Discard new worker</button></div>` : ""}
      <label>Name <input data-k="name" data-wid="${escapeAttr(w.id)}" type="text" value="${escapeAttr(w.name)}" /></label>
      ${domainSelectHtml(w)}
      <label>Model override <input data-k="modelOverride" data-wid="${escapeAttr(w.id)}" type="text" value="${escapeAttr(w.modelOverride || "")}" placeholder="gemma4:e2b" /></label>
      <label>Docker image <input data-k="dockerImage" data-wid="${escapeAttr(w.id)}" type="text" value="${escapeAttr(w.dockerImage || "")}" placeholder="local-ai-worker-agent:latest" /></label>
      <label class="enable-row"><input data-k="enabled" data-wid="${escapeAttr(w.id)}" type="checkbox" ${w.enabled ? "checked" : ""} /> Worker enabled</label>
      <label>Guardrail overrides (JSON object, merged into domain guardrails)
        <textarea data-field="guardrails" data-wid="${escapeAttr(w.id)}" rows="5" class="code">${escapeTextarea(gr)}</textarea>
      </label>
      <div class="env-bind-block">
        <div class="env-bind-label">Secrets → container environment</div>
        <div class="env-bind-rows">
          ${bindHtml || '<p class="hint">No mappings — default <code>GITHUB_TOKEN</code> still applies when configured.</p>'}
        </div>
        <div class="row">
          <button type="button" data-action="env-bind-add" data-wid="${escapeAttr(w.id)}">+ Map secret to env</button>
        </div>
      </div>
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
      <div class="docker-steps">
        <div class="docker-step">
          <span class="step-num">1</span>
          <div class="docker-step-body">
            <strong>Prepare storage</strong>
            <p class="hint">Run this first for a new worker: creates the context file, Docker volume, and agent files on disk.</p>
            <button type="button" data-docker="prepare" data-wid="${escapeAttr(w.id)}">Prepare storage</button>
          </div>
        </div>
        <div class="docker-step">
          <span class="step-num">2</span>
          <div class="docker-step-body">
            <strong>Start container</strong>
            <p class="hint">After step 1, start (or restart) the agent container. Use this after reboots too; it does not replace Prepare.</p>
            <div class="row wrap">
              <button type="button" data-docker="start" data-wid="${escapeAttr(w.id)}">Start container</button>
              <button type="button" data-docker="stop" data-wid="${escapeAttr(w.id)}" class="secondary">Stop</button>
              <button type="button" data-docker="recreate" data-wid="${escapeAttr(w.id)}" class="secondary">Recreate</button>
              <button type="button" data-docker="status" data-wid="${escapeAttr(w.id)}" class="secondary">Status</button>
              <button type="button" data-docker="logs" data-wid="${escapeAttr(w.id)}" class="secondary">Logs</button>
            </div>
          </div>
        </div>
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
      if (k === "enabled") renderWorkers(workersCache);
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
  workersCache.forEach(ensureEnvBindings);
  renderWorkers(workersCache);
}

document.getElementById("workers-list").addEventListener("change", (e) => {
  const t = e.target;
  const root = document.getElementById("workers-list");

  const kindSel = t.closest("select[data-task-field='kind']");
  if (kindSel) {
    const wid = kindSel.dataset.wid;
    const ti = Number(kindSel.dataset.taskIdx);
    const i = findWorkerIndex(wid);
    if (i < 0) return;
    const task = workersCache[i].tasks[ti];
    if (!task) return;
    if (kindSel.value === "cadence") {
      const prev = task.schedule.kind === "cadence" ? task.schedule.intervalSeconds : 3600;
      task.schedule = { kind: "cadence", intervalSeconds: prev };
    } else {
      task.schedule = { kind: "oneShot" };
    }
    renderWorkers(workersCache);
    return;
  }

  const sel = t.closest("[data-domain-select]");
  if (sel) {
    const wid = sel.dataset.wid;
    const i = findWorkerIndex(wid);
    if (i < 0) return;
    const wrap = root.querySelector(`[data-domain-other-wrap="${wid}"]`);
    const customInp = wrap?.querySelector("[data-domain-custom]");
    if (sel.value === "__other__") {
      wrap?.classList.remove("hidden");
      if (customInp) customInp.value = workersCache[i].maintenanceDomain || "";
    } else {
      workersCache[i].maintenanceDomain = sel.value;
      wrap?.classList.add("hidden");
    }
    return;
  }

  const titleInp = t.closest("input[data-task-field='title']");
  if (titleInp) {
    const wid = titleInp.dataset.wid;
    const ti = Number(titleInp.dataset.taskIdx);
    const i = findWorkerIndex(wid);
    if (i >= 0 && workersCache[i].tasks[ti]) {
      workersCache[i].tasks[ti].title = titleInp.value;
    }
    return;
  }

  const intInp = t.closest("input[data-task-field='interval']");
  if (intInp) {
    const wid = intInp.dataset.wid;
    const ti = Number(intInp.dataset.taskIdx);
    const i = findWorkerIndex(wid);
    const task = i >= 0 ? workersCache[i].tasks[ti] : null;
    if (task && task.schedule.kind === "cadence") {
      const v = Math.max(30, Number(intInp.value) || 3600);
      task.schedule.intervalSeconds = v;
      intInp.value = String(v);
    }
  }
});

document.getElementById("workers-list").addEventListener("input", (e) => {
  const inp = e.target.closest("input[data-env-bind]");
  if (inp) {
    const wid = inp.dataset.wid;
    const idx = Number(inp.dataset.envIdx);
    const part = inp.dataset.envPart;
    const i = findWorkerIndex(wid);
    if (i < 0 || idx < 0 || !workersCache[i].envFromSecrets[idx]) return;
    if (part === "envVar") workersCache[i].envFromSecrets[idx].envVar = inp.value;
    else if (part === "secretKey") workersCache[i].envFromSecrets[idx].secretKey = inp.value;
    return;
  }

  const custom = e.target.closest("[data-domain-custom]");
  if (custom) {
    const wid = custom.dataset.wid;
    const i = findWorkerIndex(wid);
    if (i < 0) return;
    workersCache[i].maintenanceDomain = custom.value.trim() || workersCache[i].maintenanceDomain;
  }
});

document.getElementById("workers-list").addEventListener("click", async (e) => {
  const t = e.target;

  if (t.closest("[data-action='show-domain-help']")) {
    openModal(document.getElementById("modal-domain"));
    return;
  }
  if (t.closest("[data-action='show-tasks-help']")) {
    openModal(document.getElementById("modal-tasks"));
    return;
  }

  const discard = t.closest("[data-action='worker-discard']");
  if (discard) {
    const wid = discard.dataset.wid;
    draftWorkerIds.delete(wid);
    workersCache = workersCache.filter((w) => w.id !== wid);
    renderWorkers(workersCache);
    return;
  }

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
  const erm = t.closest("[data-action='env-bind-remove']");
  if (erm) {
    const wid = erm.dataset.wid;
    const ei = Number(erm.dataset.envIdx);
    const i = findWorkerIndex(wid);
    if (i >= 0 && ei >= 0) {
      workersCache[i].envFromSecrets.splice(ei, 1);
      renderWorkers(workersCache);
    }
    return;
  }
  const eadd = t.closest("[data-action='env-bind-add']");
  if (eadd) {
    const wid = eadd.dataset.wid;
    const i = findWorkerIndex(wid);
    if (i >= 0) {
      ensureEnvBindings(workersCache[i]);
      workersCache[i].envFromSecrets.push({ envVar: "GITHUB_TOKEN", secretKey: "github_token" });
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

document.getElementById("btn-secret-add").addEventListener("click", async () => {
  const keyEl = document.getElementById("secret-new-key");
  const valEl = document.getElementById("secret-new-value");
  const msg = document.getElementById("secrets-msg");
  const key = keyEl.value.trim();
  const value = valEl.value;
  if (!key || !value) {
    msg.textContent = "Enter both key and value.";
    return;
  }
  try {
    await invoke("secret_set", { key, value });
    valEl.value = "";
    msg.textContent = "Saved.";
    await refreshSecretsTable();
  } catch (e) {
    msg.textContent = String(e);
  }
});

document.getElementById("secrets-tbody")?.addEventListener("click", async (e) => {
  const btn = e.target.closest("[data-secret-remove]");
  if (!btn) return;
  const key = btn.getAttribute("data-secret-remove");
  if (!key) return;
  const msg = document.getElementById("secrets-msg");
  if (!window.confirm(`Remove secret "${key}" from the keychain?`)) return;
  try {
    await invoke("secret_delete", { key });
    if (msg) msg.textContent = "Removed.";
    await refreshSecretsTable();
  } catch (err) {
    if (msg) msg.textContent = String(err);
  }
});

document.getElementById("btn-add-worker").addEventListener("click", () => {
  const id = crypto.randomUUID();
  draftWorkerIds.add(id);
  workersCache.push(workerTemplate(id));
  renderWorkers(workersCache);
});

document.getElementById("btn-save-workers").addEventListener("click", async () => {
  const msg = document.getElementById("workers-msg");
  try {
    await invoke("save_workers", { workers: workersCache });
    msg.textContent = "Saved.";
    draftWorkerIds.clear();
    await loadWorkers();
  } catch (e) {
    msg.textContent = String(e);
  }
});

async function resolveRestore(choice) {
  try {
    await invoke("session_resolve_restore", { choice });
    closeModal(document.getElementById("modal-restore"));
    await loadWorkers();
  } catch (e) {
    window.alert(String(e));
  }
}

document.getElementById("btn-restore-snapshot")?.addEventListener("click", () => resolveRestore("restoreSnapshot"));
document.getElementById("btn-restore-disable")?.addEventListener("click", () => resolveRestore("disableAll"));
document.getElementById("btn-restore-dismiss")?.addEventListener("click", () => resolveRestore("dismiss"));
document.getElementById("btn-restore-close")?.addEventListener("click", () => resolveRestore("dismiss"));
document.getElementById("modal-restore")?.addEventListener("click", (e) => {
  if (e.target === e.currentTarget) resolveRestore("dismiss");
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

async function boot() {
  await loadRulesDomains();
  refreshEnv();
  refreshComposeHint().catch(() => {});
  refreshAppLog().catch(() => {});
  refreshAudit().catch(() => {});
  refreshSecretsTable().catch((e) => {
    const m = document.getElementById("secrets-msg");
    if (m) m.textContent = String(e);
  });
  try {
    await loadWorkers();
  } catch (e) {
    document.getElementById("workers-msg").textContent = String(e);
  }
  await maybeShowRestorePrompt();
}

boot();

try {
  startUpdateCheckScheduler();
} catch {
  /* non-Tauri */
}

document.getElementById("btn-releases")?.addEventListener("click", () => {
  openReleasesPage().catch((e) => console.warn(e));
});

document.getElementById("btn-check-updates")?.addEventListener("click", async () => {
  try {
    const u = await fetchUpdateIfAvailable();
    if (!u) {
      window.alert("You're on the latest version (or update server unreachable).");
      return;
    }
    const ok = window.confirm(`Update available: ${u.version}. Install now?`);
    if (!ok) return;
    await u.downloadAndInstall();
    const { relaunch } = await import("@tauri-apps/plugin-process");
    await relaunch();
  } catch (e) {
    window.alert(String(e));
  }
});
