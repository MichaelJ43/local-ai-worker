/**
 * Bump semver in package.json, package-lock.json, root Cargo.toml [workspace.package],
 * crates/ai_worker_core/Cargo.toml, src-tauri/tauri.conf.json.
 * Usage: node scripts/bump-version.mjs patch|minor|major
 */
import { readFileSync, writeFileSync } from "node:fs";
import { execSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, "..");

const kind = process.argv[2];
if (!["patch", "minor", "major"].includes(kind)) {
  console.error("Usage: node scripts/bump-version.mjs patch|minor|major");
  process.exit(1);
}

function parseSemver(v) {
  const m = /^(\d+)\.(\d+)\.(\d+)/.exec(v.trim());
  if (!m) throw new Error(`Bad version: ${v}`);
  return { major: +m[1], minor: +m[2], patch: +m[3] };
}

function bump(v, k) {
  const { major, minor, patch } = parseSemver(v);
  if (k === "patch") return `${major}.${minor}.${patch + 1}`;
  if (k === "minor") return `${major}.${minor + 1}.0`;
  return `${major + 1}.0.0`;
}

const pkgPath = join(root, "package.json");
const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
const next = bump(pkg.version, kind);
pkg.version = next;
writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n");

const workspaceCargoPath = join(root, "Cargo.toml");
let workspaceCargo = readFileSync(workspaceCargoPath, "utf8");
workspaceCargo = workspaceCargo.replace(
  /(\[workspace\.package\]\s*\nversion = )"[^"]+"/,
  `$1"${next}"`
);
writeFileSync(workspaceCargoPath, workspaceCargo);

const coreCargoPath = join(root, "crates", "ai_worker_core", "Cargo.toml");
let coreCargo = readFileSync(coreCargoPath, "utf8");
coreCargo = coreCargo.replace(/^version = "[^"]+"/m, `version = "${next}"`);
writeFileSync(coreCargoPath, coreCargo);

const tauriPath = join(root, "src-tauri", "tauri.conf.json");
const tauri = JSON.parse(readFileSync(tauriPath, "utf8"));
tauri.version = next;
writeFileSync(tauriPath, JSON.stringify(tauri, null, 2) + "\n");

execSync("npm install --package-lock-only", { cwd: root, stdio: "inherit" });

try {
  execSync("cargo check --workspace", { cwd: root, stdio: "inherit" });
} catch {
  console.warn("cargo check skipped or failed (refresh Cargo.lock locally if needed).");
}

console.log(`Bumped to ${next}`);
