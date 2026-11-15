// ============================================================================
// Skill Studio - Set version
// `pnpm run version:set 0.3.0` writes one version into every manifest that
// check-versions.mjs reads, refreshes Cargo.lock, and runs that check.
// ============================================================================
import { execFileSync } from "node:child_process";
import { existsSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
  WORKSPACE_VERSION,
  manifests,
  platformPins,
  problems,
  repoRoot,
} from "./check-versions.mjs";

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) {
  process.stderr.write("usage: pnpm run version:set <major.minor.patch>\n");
  process.exit(1);
}

const root = repoRoot();
const writes = [];
for (const { path, file, text, json, versions } of manifests(root)) {
  if (versions.length === 0) continue;
  let next;
  if (json === undefined) {
    next = text.replace(WORKSPACE_VERSION, `$1${version}$3`);
  } else {
    next =
      json.version === undefined
        ? text
        : text.replace(/^( {2}"version":\s*")[^"]*(")/m, `$1${version}$2`);
    for (const name of Object.keys(platformPins(json))) {
      next = next.replace(new RegExp(`^( {4}"${name}":\\s*")[^"]*(")`, "m"), `$1${version}$2`);
    }
    const written = JSON.parse(next);
    const pins = Object.values(platformPins(written));
    if (
      (json.version !== undefined && written.version !== version) ||
      pins.some((pin) => pin !== version)
    ) {
      process.stderr.write(`${file}: could not set its version safely; nothing was written.\n`);
      process.exit(1);
    }
  }
  if (next !== text) writes.push([path, next]);
}
for (const [path, next] of writes) writeFileSync(path, next);
if (existsSync(join(root, "Cargo.lock"))) {
  execFileSync("cargo", ["update", "--workspace", "--quiet"], { cwd: root, stdio: "inherit" });
}

const left = problems(root, `v${version}`);
if (left.length > 0) {
  process.stderr.write(`Could not set every version:\n  ${left.join("\n  ")}\n`);
  process.exit(1);
}
process.stdout.write(`Set every version to ${version}.\n`);
