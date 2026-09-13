// Writes THIRD-PARTY-LICENSES.txt: the license of every Rust crate built
// into Dexter on Windows and every npm package bundled into its front end,
// with each distinct license text included once.
//
//   node scripts/third-party-licenses.mjs

import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const LICENSE_FILE = /^(licen[cs]e|copying|notice)([-_.].*)?$/i;

function licenseTexts(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir)
    .filter((name) => LICENSE_FILE.test(name))
    .sort()
    .map((name) => readFileSync(join(dir, name), "utf8").replace(/\r\n/g, "\n").trim());
}

/// Crates reachable from Dexter through normal (not dev) dependencies on Windows.
function rustPackages() {
  const metadata = JSON.parse(
    execFileSync(
      "cargo",
      ["metadata", "--format-version", "1", "--locked", "--filter-platform", "x86_64-pc-windows-msvc"],
      { cwd: join(root, "src-tauri"), maxBuffer: 256 * 1024 * 1024 },
    ),
  );
  const byId = new Map(metadata.packages.map((p) => [p.id, p]));
  const nodes = new Map(metadata.resolve.nodes.map((n) => [n.id, n]));
  const own = new Set(metadata.workspace_members);
  const reached = new Set();
  const queue = [...own];
  while (queue.length) {
    const id = queue.pop();
    if (reached.has(id)) continue;
    reached.add(id);
    for (const dep of nodes.get(id)?.deps ?? []) {
      if (dep.dep_kinds.some((k) => k.kind !== "dev")) queue.push(dep.pkg);
    }
  }
  return [...reached]
    .filter((id) => !own.has(id))
    .map((id) => byId.get(id))
    .map((p) => ({
      name: p.name,
      version: p.version,
      license: p.license ?? "see license file",
      url: p.repository ?? "",
      texts: licenseTexts(dirname(p.manifest_path)),
    }));
}

/// Packages bundled into the front end (dependencies, not devDependencies).
function npmPackages() {
  const lock = JSON.parse(readFileSync(join(root, "package-lock.json"), "utf8"));
  return Object.entries(lock.packages)
    .filter(([path, p]) => path && !p.dev)
    .map(([path, p]) => ({
      name: path.replace(/^.*node_modules\//, ""),
      version: p.version,
      license: p.license ?? "see license file",
      url: "",
      texts: licenseTexts(join(root, path)),
    }));
}

const packages = [...rustPackages(), ...npmPackages()].sort((a, b) =>
  a.name.localeCompare(b.name) || a.version.localeCompare(b.version),
);

// Many crates ship the same text; each distinct text is printed once.
const textIds = new Map();
const lines = [
  "Third-party software in Dexter",
  "==============================",
  "",
  "Dexter is licensed under the GNU General Public License v3.0 or later (see LICENSE).",
  "It includes the following packages, under their own licenses.",
  "",
];
for (const p of packages) {
  const refs = p.texts.map((text) => {
    if (!textIds.has(text)) textIds.set(text, textIds.size + 1);
    return `[${textIds.get(text)}]`;
  });
  lines.push(`${p.name} ${p.version} - ${p.license}${p.url ? ` - ${p.url}` : ""}${refs.length ? ` - texts ${refs.join(" ")}` : ""}`);
}
lines.push("", "", "License texts", "=============", "");
for (const [text, id] of textIds) {
  lines.push(`--- [${id}] ${"-".repeat(70)}`, "", text, "");
}
const missing = packages.filter((p) => p.texts.length === 0);
writeFileSync(join(root, "THIRD-PARTY-LICENSES.txt"), lines.join("\n") + "\n");
console.log(`${packages.length} packages, ${textIds.size} distinct license texts`);
if (missing.length) console.log(`no license file shipped by: ${missing.map((p) => `${p.name} ${p.version}`).join(", ")}`);
