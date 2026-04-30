import { invoke } from "@tauri-apps/api/core";
import {
  fetchUpdateIfAvailable,
  openReleasesPage,
  startUpdateCheckScheduler,
} from "./updater.js";
import { listen } from "@tauri-apps/api/event";

const VIEW_TITLES = {
  overview: "Overview",
  llmSources: "LLM sources",
  workers: "Workers",
  secrets: "Secrets",
  diagnostics: "Diagnostics",
};

/** @type {Array<Record<string, unknown>>} */
let llmSourcesCache = [];

async function loadLlmSources() {
  try {
    llmSourcesCache = await invoke("get_llm_sources");
  } catch {
    llmSourcesCache = [];
  }
}

/** @type {{ key: string, label: string, selectable: boolean }[]} */
let domainsCache = [];
const draftWorkerIds = new Set();

const DEFAULT_AGENT_IMAGE = "local-ai-worker-agent:latest";

/** @type {string | null} */
let expandedWorkerId = null;

/** Worker cards with Advanced section expanded (persisted across re-render). */
const expandedAdvancedWorkerIds = new Set();

/** @type {Record<string, object>} */
let lastPersistedById = {};

/** @type {Array<object>} */
let workersCache = [];

function rebuildLastPersistedSnapshots() {
  lastPersistedById = {};
  for (const w of workersCache) {
    try {
      lastPersistedById[w.id] = structuredClone(w);
    } catch {
      lastPersistedById[w.id] = JSON.parse(JSON.stringify(w));
    }
  }
}

function setRuntimeBanner(visible, message = "") {
  const bar = document.getElementById("runtime-banner");
  const txt = document.getElementById("runtime-banner-text");
  if (!bar || !txt) return;
  if (visible) {
    txt.textContent = message || "Applying worker runtime…";
    bar.classList.remove("hidden");
  } else {
    bar.classList.add("hidden");
    txt.textContent = "";
  }
}

/** @returns {Promise<() => void>} */
async function bindRuntimeListeners() {
  const unsubs = [];
  try {
    unsubs.push(
      await listen("runtime-phase", (ev) => {
        const m = typeof ev.payload === "object" && ev.payload && ev.payload.message;
        setRuntimeBanner(true, typeof m === "string" ? m : String(ev.payload));
      }),
    );
    unsubs.push(
      await listen("runtime-finished", async () => {
        setRuntimeBanner(false);
        await refreshEnv();
        await refreshAppLog();
      }),
    );
    unsubs.push(
      await listen("runtime-error", async (ev) => {
        setRuntimeBanner(false);
        const err =
          typeof ev.payload === "object" && ev.payload && ev.payload.error != null
            ? String(ev.payload.error)
            : String(ev.payload);
        const msg = document.getElementById("workers-msg");
        if (msg) msg.textContent = err;
        await refreshAppLog();
      }),
    );
  } catch {
    /* non-Tauri / tests */
  }
  return () => unsubs.forEach((u) => u());
}

function normalizeWorkerForSave(w) {
  if (!w.tasks) w.tasks = [];
  for (const t of w.tasks) {
    if (t.schedule?.kind === "cadence") {
      const sec = Number(
        t.schedule.intervalSeconds ?? t.schedule.interval_seconds,
      );
      t.schedule = {
        kind: "cadence",
        intervalSeconds: Math.max(30, Number.isFinite(sec) ? sec : 3600),
      };
    }
  }
  if (!w.dockerImage || !String(w.dockerImage).trim()) {
    w.dockerImage = DEFAULT_AGENT_IMAGE;
  }
  if (w.workerPrompt != null) {
    const t = String(w.workerPrompt).trim();
    w.workerPrompt = t || null;
  }
  if (w.hybridOptions && typeof w.hybridOptions === "object") {
    const r = w.hybridOptions.repoUrl;
    if (r != null && !String(r).trim()) w.hybridOptions.repoUrl = null;
  }
}

function normalizeAllWorkersForSave() {
  workersCache.forEach((w) => normalizeWorkerForSave(w));
}

async function persistAllWorkers(userMessage) {
  const msg = document.getElementById("workers-msg");
  normalizeAllWorkersForSave();
  if (msg && userMessage) msg.textContent = userMessage;
  try {
    const res = await invoke("save_workers", { workers: workersCache });
    if (res && res.runtimePending) {
      setRuntimeBanner(true, "Applying worker runtime…");
    }
    if (msg) msg.textContent = "Saved.";
    await loadWorkers();
  } catch (e) {
    if (msg) msg.textContent = String(e);
  }
}

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
  btn.addEventListener("click", async () => {
    showView(btn.dataset.nav);
    if (btn.dataset.nav === "llmSources") {
      await loadLlmSources();
      renderLlmSourcesEditor();
    }
  });
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
  const el = document.getElementById("compose-status-overview");
  if (!el) return;
  try {
    const h = await invoke("ollama_stack_gpu_hint");
    el.textContent = `Compose: ${h.composeDir} · nvidia-smi: ${h.nvidiaSmiAvailable ? "yes" : "no"} · auto GPU compose: ${h.autoUseGpu ? "on" : "off"} (Workers using loopback Ollama toggle the stack)`;
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
    escalationPath: [],
    modelOverride: null,
    ollamaHost: null,
    enabled: false,
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
    dockerImage: DEFAULT_AGENT_IMAGE,
    envFromSecrets: [],
    hybridOptions: null,
    workerPrompt: null,
    repoExecutionMode: "observe",
    allowedTestProfiles: null,
    dockerNetwork: null,
    repoSandboxPolicy: null,
  };
}

function escapeAttr(s) {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/"/g, "&quot;")
    .replace(/</g, "&lt;");
}

function escalationOptionsHtml() {
  let o =
    '<option value="">— choose source —</option>';
  for (const src of llmSourcesCache) {
    const lbl =
      src.kind === "cursor"
        ? `Cursor • ${escapeAttr(String(src.cursorModelId || src.id))}`
        : `Ollama • ${escapeAttr(String(src.defaultModel || src.baseUrl))}`;
    o += `<option value="${escapeAttr(src.id)}">${lbl}</option>`;
  }
  return o;
}

function escalationRowsHtml(w) {
  if (!Array.isArray(w.escalationPath)) w.escalationPath = [];

  const tiers =
    w.escalationPath.length === 0 ? [""] : w.escalationPath.slice();

  const rows = tiers
      .map(
        (sid, ix) =>
          `<div class="escalation-row row tight" data-wid="${escapeAttr(w.id)}" data-esc-idx="${ix}">
      <select data-field="tier" data-wid="${escapeAttr(w.id)}" data-esc-idx="${ix}">
        ${tierSelectOptionsHtml(sid || "")}
      </select>
      <button type="button" class="secondary small-pad" data-action="escal-up" data-wid="${escapeAttr(w.id)}" data-esc-idx="${ix}" ${ix === 0 ? "disabled" : ""}>↑</button>
      <button type="button" class="secondary small-pad" data-action="escal-down" data-wid="${escapeAttr(w.id)}" data-esc-idx="${ix}" ${ix === tiers.length - 1 ? "disabled" : ""}>↓</button>
      <button type="button" class="linkish" data-action="escal-remove" data-wid="${escapeAttr(w.id)}" data-esc-idx="${ix}">remove</button>
    </div>`,
      )
      .join("");

  return `<div class="escal-block" data-wid="${escapeAttr(w.id)}">
    <div class="task-label">Escalation path (ordered tiers)</div>
    ${rows}
    <button type="button" data-action="escal-add" data-wid="${escapeAttr(w.id)}">+ Add tier</button>
    <p class="hint">Include at least one <strong>Ollama</strong> tier to run the Docker agent; add <strong>Cursor</strong> for hybrid escalation.</p>
  </div>`;
}

function tierSelectOptionsHtml(selectedId) {
  let o =
    '<option value="">— choose source —</option>';
  for (const src of llmSourcesCache) {
    const sel = selectedId === src.id ? ' selected=""' : "";
    const lbl =
      src.kind === "cursor"
        ? `Cursor • ${escapeAttr(String(src.cursorModelId || src.id))}`
        : `Ollama • ${escapeAttr(String(src.defaultModel || src.baseUrl))}`;
    o += `<option value="${escapeAttr(src.id)}"${sel}>${lbl}</option>`;
  }
  return o;
}

function escapeTextarea(s) {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;");
}

function findWorkerIndex(wid) {
  return workersCache.findIndex((w) => w.id === wid);
}

function discardWorkerDraftOrRevert(wid) {
  const i = findWorkerIndex(wid);
  if (i < 0) return;
  if (draftWorkerIds.has(wid)) {
    draftWorkerIds.delete(wid);
    expandedAdvancedWorkerIds.delete(wid);
    if (expandedWorkerId === wid) expandedWorkerId = null;
    workersCache.splice(i, 1);
    renderWorkers(workersCache);
    return;
  }
  const snap = lastPersistedById[wid];
  if (snap) {
    try {
      workersCache[i] = structuredClone(snap);
    } catch {
      workersCache[i] = JSON.parse(JSON.stringify(snap));
    }
    renderWorkers(workersCache);
  } else {
    void loadWorkers();
  }
}

function ensureEnvBindings(w) {
  if (!Array.isArray(w.envFromSecrets)) w.envFromSecrets = [];
}

function normalizeRepoExecMode(raw) {
  const t = String(raw ?? "").trim();
  if (t === "apply_git" || t === "apply_github") return t;
  return "observe";
}

/** Coalesce allowed test-runner profiles saved on disk. */
function ensureWorkerAllowedTestProfiles(w) {
  const p = w.allowedTestProfiles;
  if (p == null) return;
  if (!Array.isArray(p)) {
    w.allowedTestProfiles = null;
    return;
  }
  w.allowedTestProfiles = p
    .filter((x) => x && typeof x === "object")
    .map((x) => ({
      command: String(x.command ?? "").trim(),
      argv: Array.isArray(x.argv) ? x.argv.map((a) => String(a)) : [],
    }))
    .filter((x) => x.command.length > 0);
  if (w.allowedTestProfiles.length === 0) w.allowedTestProfiles = null;
}

function stringifyAllowedTestProfiles(w) {
  if (!Array.isArray(w.allowedTestProfiles) || w.allowedTestProfiles.length === 0) {
    return '[\n  { "command": "npm", "argv": ["test"] }\n]\n';
  }
  try {
    return JSON.stringify(w.allowedTestProfiles, null, 2);
  } catch {
    return "[\n]\n";
  }
}

function defaultRepoSandboxObject() {
  return {
    fileWritesEnabled: false,
    applyUnifiedDiffsEnabled: false,
    patchMaxFileBytes: 393216,
    patchMaxTotalBytes: 3145728,
    patchMaxUnifiedDiffBytes: 1048576,
    allowGitInternalsWrites: false,
  };
}

function stringifyRepoSandboxPolicy(w) {
  const o =
    w.repoSandboxPolicy &&
    typeof w.repoSandboxPolicy === "object" &&
    !Array.isArray(w.repoSandboxPolicy)
      ? w.repoSandboxPolicy
      : defaultRepoSandboxObject();
  try {
    return JSON.stringify(o, null, 2);
  } catch {
    return JSON.stringify(defaultRepoSandboxObject(), null, 2);
  }
}

function ensureRepoSandboxShape(w) {
  const p = w.repoSandboxPolicy;
  if (
    p !== null &&
    typeof p === "object" &&
    !Array.isArray(p)
  ) {
    return;
  }
  w.repoSandboxPolicy = null;
}

/** Normalize repo execution tier for editor + JSON schema. */
function ensureWorkerDefaults(w) {
  w.repoExecutionMode = normalizeRepoExecMode(w.repoExecutionMode);
  ensureWorkerAllowedTestProfiles(w);
  ensureRepoSandboxShape(w);
}

/** Default hybrid options mirror Rust `HybridOptions`. */
function defaultHybridOptions() {
  return {
    cursorSecretKey: null,
    repoUrl: null,
    startingRef: null,
    workspacePath: null,
    allowCloudEscalation: true,
    localPhaseTimeoutMs: null,
    localMaxAttempts: null,
    cursorModelId: null,
  };
}

/** Ensure `hybridOptions` exists for editor + save payloads. */
function ensureHybridOptions(w) {
  if (!w.hybridOptions || typeof w.hybridOptions !== "object") {
    w.hybridOptions = defaultHybridOptions();
    return;
  }
  const d = defaultHybridOptions();
  for (const k of Object.keys(d)) {
    if (w.hybridOptions[k] === undefined) w.hybridOptions[k] = d[k];
  }
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
    ensureHybridOptions(w);
    ensureWorkerDefaults(w);
    const gr =
      w.guardrailOverrides != null
        ? JSON.stringify(w.guardrailOverrides, null, 2)
        : "";
    const tasksHtml = w.tasks
      .map((t, ti) => {
        const isCad = t.schedule.kind === "cadence";
        const intv = isCad
          ? Number(t.schedule.intervalSeconds ?? t.schedule.interval_seconds) || 3600
          : 3600;
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
    const expanded = expandedWorkerId === w.id || draftWorkerIds.has(w.id);
    const dockerDisplay =
      w.dockerImage && String(w.dockerImage).trim()
        ? String(w.dockerImage).trim()
        : DEFAULT_AGENT_IMAGE;

    const card = document.createElement("div");
    card.className = [
      "worker-card",
      w.enabled ? "worker-card--on" : "worker-card--off",
      expanded ? "" : "worker-card--collapsed",
    ]
      .filter(Boolean)
      .join(" ");

    card.dataset.workerId = w.id;

    let body = "";
    if (isDraft) {
      body += `<div class="row"><button type="button" class="secondary" data-action="worker-discard-new" data-wid="${escapeAttr(w.id)}">Discard new worker</button></div>`;
    }
    body += `<div class="worker-summary">
      <div class="worker-summary-meta">
        <strong>${escapeAttr(w.name)}</strong>
        <code class="wid" title="${escapeAttr(w.id)}">${escapeAttr(w.id.slice(0, 8))}…</code>
        <span class="worker-status ${w.enabled ? "worker-status--on" : "worker-status--off"}">${w.enabled ? "Enabled" : "Disabled"}</span>
      </div>
      <div class="worker-summary-actions">
        <label class="inline tight"><input type="checkbox" data-summary-enable data-wid="${escapeAttr(w.id)}" ${w.enabled ? "checked" : ""} /> Enabled</label>
        <button type="button" class="secondary" data-action="worker-edit-config" data-wid="${escapeAttr(w.id)}">${expanded ? "Collapse" : "Edit config"}</button>
        <button type="button" class="secondary" data-action="worker-delete" data-wid="${escapeAttr(w.id)}" ${w.enabled ? 'disabled="" title="Disable first"' : ""}>Remove</button>
      </div>
    </div>`;

    if (expanded) {
      body += `<div class="worker-detail"><label>Name <input data-k="name" data-wid="${escapeAttr(w.id)}" type="text" value="${escapeAttr(w.name)}" /></label>`;
      body += domainSelectHtml(w);
      ensureHybridOptions(w);
      const repoDisp = String(w.hybridOptions?.repoUrl ?? "").trim();
      const refDisp = String(w.hybridOptions?.startingRef ?? "").trim();
      const execDisp = normalizeRepoExecMode(w.repoExecutionMode);
      body += `<label>GitHub repository URL
        <input type="text" autocomplete="off" data-worker-repo data-wid="${escapeAttr(w.id)}" value="${escapeAttr(repoDisp)}" placeholder="https://github.com/org/repo" />
      </label>
      <label>Starting branch or ref <span class="hint block">Clone/checkout hint (branch name, tag, or commit-ish).</span>
        <input type="text" autocomplete="off" data-worker-starting-ref data-wid="${escapeAttr(w.id)}" value="${escapeAttr(refDisp)}" placeholder="main" />
      </label>
      <label>Worker prompt
        <span class="hint block">Merged into <code>system-prompt.txt</code> when runtime is prepared (after domain guardrails).</span>
        <textarea data-worker-prompt data-wid="${escapeAttr(w.id)}" rows="8" class="code">${escapeTextarea(w.workerPrompt ?? "")}</textarea>
      </label>`;
      body += escalationRowsHtml(w);
      body += `<div class="env-bind-block">
        <div class="env-bind-label">Secrets → container environment</div>
        <div class="env-bind-rows">
          ${bindHtml || '<p class="hint">No mappings — default <code>GITHUB_TOKEN</code> still applies when configured.</p>'}
        </div>
        <div class="row">
          <button type="button" data-action="env-bind-add" data-wid="${escapeAttr(w.id)}">+ Map secret to env</button>
        </div>
      </div>`;
      body += `<div class="task-block">
        <div class="task-label">Tasks</div>
        ${tasksHtml || '<p class="hint">No tasks</p>'}
        <div class="row">
          <button type="button" data-action="task-cadence" data-wid="${escapeAttr(w.id)}">+ Cadence</button>
          <button type="button" data-action="task-oneshot" data-wid="${escapeAttr(w.id)}">+ One-shot</button>
        </div>
      </div>`;
      body += `<div class="paths hint">
        Context: ${w.contextPath ? escapeAttr(w.contextPath) : "— (run Prepare under Advanced)"}<br/>
        Volume: ${w.longTermVolume ? escapeAttr(w.longTermVolume) : "—"}${repoDisp ? `<br/><span class="muted">With a repo URL configured, diagnostics also append to <code>repo-agent-runtime.log</code> beside the files above.</span>` : ""}
      </div>`;

      const advOpen = expandedAdvancedWorkerIds.has(w.id);
      body += `<details class="worker-advanced"${advOpen ? " open" : ""} data-wid="${escapeAttr(w.id)}">
        <summary class="worker-advanced-summary">
          <span class="worker-advanced-summary__label">Advanced settings</span>
          <span class="worker-advanced-switch" aria-hidden="true"><span class="worker-advanced-switch__knob"></span></span>
        </summary>
        <div class="worker-advanced-panel">
          <p class="hint muted">Docker image, JSON guardrail overrides, and manual container actions (these can change runtime outside Save).</p>
          <label>Repo autonomous execution tier
            <span class="hint block"><code>observe</code> — propose only.<br/><code>apply_git</code> — run model-proposed <code>git</code> through guarded wrappers (<code>gh</code> skipped).<br/><code>apply_github</code> — run guarded <code>git</code> and <code>gh</code> proposals.</span>
            <select data-repo-exec-mode data-wid="${escapeAttr(w.id)}">
              <option value="observe" ${execDisp === "observe" ? 'selected=""' : ""}>observe</option>
              <option value="apply_git" ${execDisp === "apply_git" ? 'selected=""' : ""}>apply_git</option>
              <option value="apply_github" ${execDisp === "apply_github" ? 'selected=""' : ""}>apply_github</option>
            </select>
          </label>
          <label>Allowed test profiles (JSON)
            <span class="hint block">Optional array executed from <code>REPO_ROOT</code> each repo cycle after model commands — only when tier is <code>apply_git</code> or <code>apply_github</code>. Each object uses <code>command</code> (bare PATH name only) and <code>argv</code> (string array). Example: <code>npm</code> with argv <code>run</code>, <code>test</code>. Install tools in your custom Docker image if needed.</span>
            <textarea data-allowed-test-profiles data-wid="${escapeAttr(w.id)}" rows="5" class="code">${escapeTextarea(stringifyAllowedTestProfiles(w))}</textarea>
          </label>
          <label>Docker network (optional <code>--network</code>)
            <span class="hint block">Docker network name (<code>bridge</code>, <code>none</code>, or a network you created). Defaults to Docker daemon default if empty. Tradeoffs in <code>docs/REPO_AGENT_SANDBOX.md</code>.</span>
            <input type="text" data-k="dockerNetwork" data-wid="${escapeAttr(w.id)}" value="${escapeAttr(w.dockerNetwork ?? "")}" placeholder="(default bridge)" autocomplete="off" />
          </label>
          <label>Repo sandbox policy (JSON)
            <span class="hint block">Controls model <code>fileWrites</code> and <code>unifiedDiffs</code>. Turn on <code>fileWritesEnabled</code> or <code>applyUnifiedDiffsEnabled</code> only for experimental runs. Bounds: <code>patchMax*</code> bytes.</span>
            <textarea data-repo-sandbox-policy data-wid="${escapeAttr(w.id)}" rows="6" class="code">${escapeTextarea(stringifyRepoSandboxPolicy(w))}</textarea>
          </label>
          <label>Docker image <input data-k="dockerImage" data-wid="${escapeAttr(w.id)}" type="text" value="${escapeAttr(dockerDisplay)}" placeholder="${escapeAttr(DEFAULT_AGENT_IMAGE)}" /></label>
          <p data-worker-lifecycle-msg class="muted" data-wid="${escapeAttr(w.id)}"></p>
          <label>Guardrail overrides (JSON object, merged into domain guardrails)
            <textarea data-field="guardrails" data-wid="${escapeAttr(w.id)}" rows="5" class="code">${escapeTextarea(gr)}</textarea>
          </label>
          <div class="docker-tools">
            <div class="task-label">Docker (manual)</div>
            <p class="hint muted">Prepare/start/stop can trigger saves and compose side effects.</p>
            <div class="row wrap">
              <button type="button" data-docker="prepare" data-wid="${escapeAttr(w.id)}" class="secondary">Prepare storage</button>
              <button type="button" data-docker="start" data-wid="${escapeAttr(w.id)}" class="secondary">Force start</button>
              <button type="button" data-docker="stop" data-wid="${escapeAttr(w.id)}" class="secondary">Force stop</button>
              <button type="button" data-docker="status" data-wid="${escapeAttr(w.id)}">Status</button>
              <button type="button" data-docker="logs" data-wid="${escapeAttr(w.id)}" class="secondary">Logs</button>
            </div>
          </div>
          <pre class="worker-docker-out preview small" data-wid="${escapeAttr(w.id)}"></pre>
        </div>
      </details>`;

      body += `<div class="worker-save-row">
        <button type="button" data-action="worker-save-one" data-wid="${escapeAttr(w.id)}">Save worker config</button>
        <button type="button" class="secondary" data-action="worker-discard-one" data-wid="${escapeAttr(w.id)}">${draftWorkerIds.has(w.id) ? "Discard new worker" : "Discard changes"}</button>
      </div></div>`;
    } else {
      body += `<p class="hint muted">Edit config to change tasks, escalation, and Docker settings.</p>`;
    }

    card.innerHTML = body;
    root.appendChild(card);
  });

  root.querySelectorAll("input[data-k]").forEach((inp) => {
    inp.addEventListener("change", () => {
      const wid = inp.dataset.wid;
      const i = findWorkerIndex(wid);
      if (i < 0) return;
      const k = inp.dataset.k;
      if (k === "dockerImage")
        workersCache[i].dockerImage = inp.value.trim() || null;
      else if (k === "dockerNetwork")
        workersCache[i].dockerNetwork = inp.value.trim() || null;
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

async function loadWorkers() {
  await loadLlmSources();
  workersCache = await invoke("get_workers");
  workersCache.forEach((w) => {
    if (!Array.isArray(w.escalationPath)) w.escalationPath = [];
  });
  workersCache.forEach(ensureEnvBindings);
  workersCache.forEach(ensureHybridOptions);
  workersCache.forEach(ensureWorkerDefaults);
  const persistedIds = new Set(workersCache.map((w) => w.id));
  for (const id of persistedIds) draftWorkerIds.delete(id);
  if (expandedWorkerId && !persistedIds.has(expandedWorkerId)) expandedWorkerId = null;
  for (const aid of [...expandedAdvancedWorkerIds]) {
    if (!persistedIds.has(aid)) expandedAdvancedWorkerIds.delete(aid);
  }
  rebuildLastPersistedSnapshots();
  renderWorkers(workersCache);
}

async function populateOllamaDefaultModelSelects() {
  const editor = document.getElementById("llm-sources-editor");
  if (!editor) return;
  const cards = editor.querySelectorAll('.llm-src-card[data-kind="ollama"]');
  for (const card of cards) {
    const ix = Number(card.dataset.index);
    const s = llmSourcesCache[ix];
    const sel = card.querySelector('select[data-part="defaultModel"]');
    if (!sel || !s || s.kind !== "ollama") continue;
    const host = String(s.baseUrl || "").trim() || null;
    const prev = String(s.defaultModel || "").trim();
    try {
      const tags = await invoke("ollama_list_models", { host });
      let opts = "";
      for (const t of tags) {
        opts += `<option value="${escapeAttr(t)}"${t === prev ? " selected" : ""}>${escapeAttr(t)}</option>`;
      }
      if (prev && tags.indexOf(prev) === -1) {
        opts = `<option value="${escapeAttr(prev)}" selected>${escapeAttr(prev)} (unlisted)</option>${opts}`;
      }
      if (!opts) opts = '<option value="">— no models —</option>';
      sel.innerHTML = opts;
      if (!prev && sel.options.length) sel.selectedIndex = 0;
      if (typeof sel.value === "string") s.defaultModel = sel.value;
    } catch {
      sel.innerHTML = `<option value="${escapeAttr(prev)}">${escapeAttr(prev || "(enter base URL)")}</option>`;
    }
  }
}

function renderLlmSourcesEditor() {
  const el = document.getElementById("llm-sources-editor");
  if (!el) return;
  if (llmSourcesCache.length === 0) {
    el.innerHTML =
      '<p class="hint">No sources yet — add an Ollama endpoint and (optionally) a Cursor source.</p>';
    return;
  }
  el.innerHTML = llmSourcesCache
    .map((src, i) => {
      if (src.kind === "ollama") {
        return `<div class="panel tight llm-src-card" data-kind="ollama" data-index="${i}">
          <div class="row spaced"><strong>Ollama source</strong><button type="button" class="secondary small-pad" data-remove-llm="${i}">Remove</button></div>
          <label>Name <input type="text" data-llm-kind="ollama" data-part="name" data-index="${i}" value="${escapeAttr(src.name || "")}" /></label>
          <label>Base URL <input type="text" data-part="baseUrl" data-index="${i}" value="${escapeAttr(src.baseUrl || "")}" /></label>
          <label>Default model <select data-part="defaultModel" data-index="${i}"><option value="${escapeAttr(String(src.defaultModel || ""))}">Loading…</option></select></label>
        </div>`;
      }
      if (src.kind === "cursor") {
        return `<div class="panel tight llm-src-card" data-kind="cursor" data-index="${i}" data-testid="llm-cursor-source-card">
          <div class="row spaced"><strong>Cursor source</strong><button type="button" class="secondary small-pad" data-remove-llm="${i}">Remove</button></div>
          <label>Name <input type="text" data-part="name" data-index="${i}" value="${escapeAttr(src.name || "")}" /></label>
          <label>Cursor model id <input type="text" data-part="cursorModelId" data-index="${i}" value="${escapeAttr(src.cursorModelId || "")}" placeholder="composer-2" /></label>
          <label>Secret key name (keychain)<input type="text" data-part="secretKeyName" data-index="${i}" list="secret-keys-datalist" value="${escapeAttr(src.secretKeyName || "cursor_api_key")}" /></label>
          <label>New API key value (optional; stored in OS keychain on Save)
            <input type="password" data-part="newSecretValue" data-index="${i}" autocomplete="off" placeholder="••••••••" />
          </label>
          <p class="hint">After saving, this field is cleared — the raw key is never persisted in JSON.</p>
        </div>`;
      }
      return "";
    })
    .join("");
  void populateOllamaDefaultModelSelects();
}

document.getElementById("btn-llm-add-ollama")?.addEventListener("click", () => {
  llmSourcesCache.push({
    kind: "ollama",
    id: crypto.randomUUID(),
    name: "Local Ollama",
    baseUrl: "http://127.0.0.1:11434",
    defaultModel: "gemma3:latest",
  });
  renderLlmSourcesEditor();
});

document.getElementById("btn-llm-add-cursor")?.addEventListener("click", () => {
  llmSourcesCache.push({
    kind: "cursor",
    id: crypto.randomUUID(),
    name: "Cursor",
    cursorModelId: "",
    secretKeyName: "cursor_api_key",
  });
  renderLlmSourcesEditor();
});

document.getElementById("llm-sources-editor")?.addEventListener("input", (ev) => {
  const inp = ev.target.closest("input[data-part][data-index]");
  if (!inp) return;
  const ix = Number(inp.dataset.index);
  const part = inp.dataset.part;
  const s = llmSourcesCache[ix];
  if (!s || !part || part === "newSecretValue") return;
  s[part] = inp.value;
});

document.getElementById("llm-sources-editor")?.addEventListener("change", async (ev) => {
  const el = ev.target.closest("[data-part][data-index]");
  if (!el) return;
  const ix = Number(el.dataset.index);
  const part = el.dataset.part;
  const s = llmSourcesCache[ix];
  if (!s || !part || part === "newSecretValue") return;
  if (el.tagName === "SELECT") {
    s[part] = el.value;
  } else if (el.tagName === "INPUT") {
    s[part] = el.value;
  }
  if (s.kind === "ollama" && part === "baseUrl") {
    await populateOllamaDefaultModelSelects();
  }
});

document.getElementById("llm-sources-editor")?.addEventListener("click", (ev) => {
  const rm = ev.target.closest("[data-remove-llm]");
  if (!rm) return;
  const ix = Number(rm.getAttribute("data-remove-llm"));
  if (!Number.isFinite(ix)) return;
  llmSourcesCache.splice(ix, 1);
  renderLlmSourcesEditor();
});

document.getElementById("btn-llm-sources-save")?.addEventListener("click", async () => {
  const msg = document.getElementById("llm-sources-msg");
  try {
    msg.textContent = "Saving…";
    /** persist any new Cursor keys */
    for (let i = 0; i < llmSourcesCache.length; i++) {
      const s = llmSourcesCache[i];
      if (s.kind !== "cursor") continue;
      const pw = document.querySelector(`input[data-part="newSecretValue"][data-index="${i}"]`);
      if (pw && pw.value.trim()) {
        const key = s.secretKeyName?.trim() || "cursor_api_key";
        await invoke("secret_set", { key, value: pw.value });
        pw.value = "";
      }
    }
    await invoke("save_llm_sources", { sources: llmSourcesCache });
    msg.textContent = "Saved LLM sources.";
    await loadLlmSources();
    renderLlmSourcesEditor();
    await loadWorkers();
  } catch (e) {
    msg.textContent = String(e);
  }
});

document.getElementById("workers-list").addEventListener("toggle", (e) => {
  const det = e.target;
  if (!(det instanceof HTMLDetailsElement) || !det.classList.contains("worker-advanced"))
    return;
  const wid = det.dataset.wid;
  if (!wid) return;
  if (det.open) expandedAdvancedWorkerIds.add(wid);
  else expandedAdvancedWorkerIds.delete(wid);
});

document.getElementById("workers-list").addEventListener("change", (e) => {
  const summaryEn = e.target.closest("[data-summary-enable]");
  if (summaryEn) {
    const wid = summaryEn.dataset.wid;
    const i = findWorkerIndex(wid);
    if (i >= 0) {
      workersCache[i].enabled = summaryEn.checked;
      void persistAllWorkers("");
    }
    return;
  }

  const repoExecSel = e.target.closest("select[data-repo-exec-mode]");
  if (repoExecSel) {
    const wid = repoExecSel.dataset.wid;
    const i = findWorkerIndex(wid);
    if (i >= 0) workersCache[i].repoExecutionMode = normalizeRepoExecMode(repoExecSel.value);
    return;
  }

  const tierSel = e.target.closest("select[data-field='tier']");
  if (tierSel) {
    const wid = tierSel.dataset.wid;
    const ix = Number(tierSel.dataset.escIdx);
    const i = findWorkerIndex(wid);
    if (i >= 0) {
      if (!Array.isArray(workersCache[i].escalationPath)) workersCache[i].escalationPath = [];
      while (workersCache[i].escalationPath.length <= ix) {
        workersCache[i].escalationPath.push("");
      }
      workersCache[i].escalationPath[ix] = tierSel.value;
    }
    return;
  }

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
  const rsp = e.target.closest("textarea[data-repo-sandbox-policy]");
  if (rsp) {
    const wid = rsp.dataset.wid;
    const i = findWorkerIndex(wid);
    const msg = document.getElementById("workers-msg");
    if (i >= 0) {
      const raw = rsp.value.trim();
      if (!raw) {
        workersCache[i].repoSandboxPolicy = null;
        if (msg) msg.textContent = "";
        return;
      }
      try {
        const parsed = JSON.parse(raw);
        if (typeof parsed === "object" && parsed !== null && !Array.isArray(parsed)) {
          workersCache[i].repoSandboxPolicy = parsed;
          ensureRepoSandboxShape(workersCache[i]);
        } else {
          workersCache[i].repoSandboxPolicy = null;
        }
        if (msg) msg.textContent = "";
      } catch (err) {
        if (msg)
          msg.textContent = `Sandbox policy: invalid JSON — ${String(err.message || err)}`;
      }
    }
    return;
  }

  const atp = e.target.closest("textarea[data-allowed-test-profiles]");
  if (atp) {
    const wid = atp.dataset.wid;
    const i = findWorkerIndex(wid);
    if (i >= 0) {
      const raw = atp.value.trim();
      const msg = document.getElementById("workers-msg");
      if (!raw) {
        workersCache[i].allowedTestProfiles = null;
        if (msg) msg.textContent = "";
        return;
      }
      try {
        const parsed = JSON.parse(raw);
        workersCache[i].allowedTestProfiles = Array.isArray(parsed) ? parsed : null;
        ensureWorkerAllowedTestProfiles(workersCache[i]);
        if (msg) msg.textContent = "";
      } catch (err) {
        if (msg) msg.textContent = `Allowed test profiles: invalid JSON — ${String(err.message || err)}`;
      }
    }
    return;
  }

  const wp = e.target.closest("textarea[data-worker-prompt]");
  if (wp) {
    const wid = wp.dataset.wid;
    const i = findWorkerIndex(wid);
    if (i >= 0) {
      const t = wp.value.trim();
      workersCache[i].workerPrompt = t ? t : null;
    }
    return;
  }

  const wrepo = e.target.closest("input[data-worker-repo]");
  if (wrepo) {
    const wid = wrepo.dataset.wid;
    const i = findWorkerIndex(wid);
    if (i >= 0) {
      ensureHybridOptions(workersCache[i]);
      const t = wrepo.value.trim();
      workersCache[i].hybridOptions.repoUrl = t || null;
    }
    return;
  }

  const wsref = e.target.closest("input[data-worker-starting-ref]");
  if (wsref) {
    const wid = wsref.dataset.wid;
    const i = findWorkerIndex(wid);
    if (i >= 0) {
      ensureHybridOptions(workersCache[i]);
      const t = wsref.value.trim();
      workersCache[i].hybridOptions.startingRef = t || null;
    }
    return;
  }

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
    return;
  }

  const intInp = e.target.closest("input[data-task-field='interval']");
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

  const editCfg = t.closest("[data-action='worker-edit-config']");
  if (editCfg) {
    const wid = editCfg.dataset.wid;
    expandedWorkerId = expandedWorkerId === wid ? null : wid;
    renderWorkers(workersCache);
    return;
  }

  const saveOne = t.closest("[data-action='worker-save-one']");
  if (saveOne) {
    await persistAllWorkers("Saved worker.");
    return;
  }

  const discOne = t.closest("[data-action='worker-discard-one']");
  const discNew = t.closest("[data-action='worker-discard-new']");
  if ((discOne || discNew)?.dataset?.wid) {
    discardWorkerDraftOrRevert((discOne || discNew).dataset.wid);
    return;
  }

  const wdel = t.closest("[data-action='worker-delete']");
  if (wdel && !wdel.disabled) {
    const wid = wdel.dataset.wid;
    if (
      !window.confirm(
        "Delete this worker? Its Docker workspace and volumes are removed. Disabled workers only.",
      )
    )
      return;
    const msg = document.getElementById("workers-msg");
    try {
      await invoke("delete_worker", { workerId: wid });
      draftWorkerIds.delete(wid);
      expandedAdvancedWorkerIds.delete(wid);
      await loadWorkers();
      if (msg) msg.textContent = "Worker deleted.";
    } catch (err) {
      if (msg) msg.textContent = String(err);
    }
    return;
  }

  const eaddTier = t.closest("[data-action='escal-add']");
  if (eaddTier) {
    const wid = eaddTier.dataset.wid;
    const i = findWorkerIndex(wid);
    if (i >= 0) {
      if (!Array.isArray(workersCache[i].escalationPath)) workersCache[i].escalationPath = [];
      workersCache[i].escalationPath.push("");
      renderWorkers(workersCache);
    }
    return;
  }

  const erem = t.closest("[data-action='escal-remove']");
  if (erem) {
    const wid = erem.dataset.wid;
    const ix = Number(erem.dataset.escIdx);
    const i = findWorkerIndex(wid);
    if (i >= 0 && workersCache[i].escalationPath) {
      workersCache[i].escalationPath.splice(ix, 1);
      renderWorkers(workersCache);
    }
    return;
  }

  const eup = t.closest("[data-action='escal-up']");
  if (eup && !eup.disabled) {
    const wid = eup.dataset.wid;
    const ix = Number(eup.dataset.escIdx);
    const i = findWorkerIndex(wid);
    const ep = workersCache[i]?.escalationPath;
    if (i >= 0 && ep && ix > 0) {
      [ep[ix - 1], ep[ix]] = [ep[ix], ep[ix - 1]];
      renderWorkers(workersCache);
    }
    return;
  }

  const edn = t.closest("[data-action='escal-down']");
  if (edn && !edn.disabled) {
    const wid = edn.dataset.wid;
    const ix = Number(edn.dataset.escIdx);
    const i = findWorkerIndex(wid);
    const ep = workersCache[i]?.escalationPath;
    if (i >= 0 && ep && ix < ep.length - 1) {
      [ep[ix], ep[ix + 1]] = [ep[ix + 1], ep[ix]];
      renderWorkers(workersCache);
    }
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
  expandedWorkerId = id;
  workersCache.push(workerTemplate(id));
  renderWorkers(workersCache);
});

async function resolveRestore(choice) {
  try {
    const res = await invoke("session_resolve_restore", { choice });
    closeModal(document.getElementById("modal-restore"));
    if (res && res.runtimePending) {
      setRuntimeBanner(true, "Applying worker runtime…");
    }
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

document.getElementById("btn-audit-refresh").addEventListener("click", refreshAudit);
document.getElementById("btn-app-log-refresh").addEventListener("click", refreshAppLog);

async function boot() {
  await bindRuntimeListeners().catch(() => {});
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
    renderLlmSourcesEditor();
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
