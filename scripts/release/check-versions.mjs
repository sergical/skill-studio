// ============================================================================
// Skill Studio - Version check
// Every shipped part (app, CLI, MCP server, npm packages, workspace packages)
// carries one version. Fails when any manifest differs from the Cargo
// workspace version, or, given a tag, when that version is not the tag's
// base version: "v0.2.0-rc.1" expects a committed "0.2.0". The full
// pre-release version is stamped in only at release time (see release.yml).
// ============================================================================
import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const SKIPPED_DIRS = new Set([
  ".git",
  ".claude",
  ".scratch",
  "node_modules",
  "target",
  "dist",
  "mutants.out",
]);
const PLATFORM_PREFIX = "@skill-studio/cli-";
export const WORKSPACE_VERSION = /(\[workspace\.package\][^[]*?^version\s*=\s*")([^"]*)(")/m;

export function repoRoot() {
  return process.env.VERSIONS_ROOT ?? fileURLToPath(new URL("../..", import.meta.url));
}

function manifestPaths(root, dir = root, out = []) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) {
      if (!SKIPPED_DIRS.has(entry.name)) manifestPaths(root, path, out);
    } else if (["package.json", "Cargo.toml", "tauri.conf.json"].includes(entry.name)) {
      out.push(path);
    }
  }
  return out;
}

/** Each file that holds a version, with the versions it holds. */
export function manifests(root = repoRoot()) {
  return manifestPaths(root).map((path) => {
    const text = readFileSync(path, "utf8");
    const file = relative(root, path);
    const versions = [];
    let json;
    if (path.endsWith("Cargo.toml")) {
      const workspace = text.match(WORKSPACE_VERSION);
      if (workspace) versions.push(workspace[2]);
      else if (/^\[package\]/m.test(text) && !/^version\.workspace\s*=\s*true/m.test(text)) {
        versions.push("not `version.workspace = true`");
      }
    } else {
      json = JSON.parse(text);
      if (json.version !== undefined) versions.push(json.version);
      versions.push(...Object.values(platformPins(json)));
    }
    return { path, file, text, json, versions };
  });
}

/** The launcher's pins on the per-platform CLI packages. */
export function platformPins(json) {
  return Object.fromEntries(
    Object.entries(json.optionalDependencies ?? {}).filter(([name]) =>
      name.startsWith(PLATFORM_PREFIX),
    ),
  );
}

export function problems(root = repoRoot(), tag) {
  const all = manifests(root);
  const cargo = all.find((m) => m.file === "Cargo.toml")?.versions[0];
  if (!cargo) return ["Cargo.toml has no [workspace.package] version"];
  const expected = tag === undefined ? cargo : tag.replace(/^v/, "").split("-")[0];
  const found = [];
  const launcher = all.find(
    (m) => m.json?.name === "skill-studio" && m.file.startsWith(join("packages", "npm")),
  );
  for (const { json } of all) {
    if (json?.name?.startsWith(PLATFORM_PREFIX) && !platformPins(launcher?.json ?? {})[json.name]) {
      found.push(`${launcher?.file ?? "the skill-studio package"}: does not pin ${json.name}`);
    }
  }
  for (const { file, versions } of all) {
    for (const version of versions) {
      if (version !== expected) found.push(`${file}: ${version}, expected ${expected}`);
    }
  }
  return found;
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const found = problems(repoRoot(), process.argv[2]);
  if (found.length > 0) {
    process.stderr.write(`Versions do not match:\n  ${found.join("\n  ")}\n`);
    process.stderr.write("Set them all with: pnpm run version:set <version>\n");
    process.exit(1);
  }
  process.stdout.write("All versions match.\n");
}
