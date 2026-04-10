import { invoke, isTauri } from "@tauri-apps/api/core";

const HOUR_MS = 60 * 60 * 1000;
export const RELEASES_LATEST_URL =
  "https://github.com/MichaelJ43/local-ai-worker/releases/latest";

export async function fetchUpdateIfAvailable() {
  if (!isTauri()) return null;
  const { check } = await import("@tauri-apps/plugin-updater");
  return await check();
}

export async function openReleasesPage() {
  if (isTauri()) {
    await invoke("open_external_url", { url: RELEASES_LATEST_URL });
    return;
  }
  window.open(RELEASES_LATEST_URL, "_blank", "noopener,noreferrer");
}

/**
 * Hourly check + one at start. Prompts via `confirm` when an update exists.
 */
export function startUpdateCheckScheduler(onCheckError) {
  if (!isTauri()) {
    return { checkNow: async () => null, dispose: () => {} };
  }

  let disposed = false;

  const tick = async () => {
    if (disposed) return;
    try {
      const u = await fetchUpdateIfAvailable();
      if (!u) return;
      const ok = window.confirm(
        `Update available: ${u.version}. Download and install now?`
      );
      if (!ok) return;
      await u.downloadAndInstall();
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    } catch (e) {
      console.warn("[updater]", e);
      onCheckError?.(e);
    }
  };

  void tick();
  const id = setInterval(() => void tick(), HOUR_MS);

  return {
    checkNow: () => fetchUpdateIfAvailable(),
    dispose: () => {
      disposed = true;
      clearInterval(id);
    },
  };
}
