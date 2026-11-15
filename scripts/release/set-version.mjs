// ============================================================================
// Skill Studio - Set version
// `pnpm run version:set 0.3.0` writes one version into every manifest that
// check-versions.mjs reads, refreshes Cargo.lock, and runs that check.
// ============================================================================
import { execFileSync } from "node:child_process";
import { existsSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { CLI_PIN, WORKSPACE_VERSION, manifests, problems, repoRoot } from "./check-versions.mjs";

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) {
  process.stderr.write("usage: pnpm run version:set <major.minor.patch>\n");
  process.exit(1);
}

const root = repoRoot();
for (const { path, text, versions } of manifests(root)) {
  if (versions.length === 0) continue;
  const next = path.endsWith("Cargo.toml")
    ? text.replace(WORKSPACE_VERSION, `$1${version}$3`)
    : text
        .replace(/^(\s*"version":\s*")[^"]*(")/m, `$1${version}$2`)
        .replace(CLI_PIN, `$1${version}$3`);
  if (next !== text) writeFileSync(path, next);
}
if (existsSync(join(root, "Cargo.lock"))) {
  execFileSync("cargo", ["update", "--workspace", "--quiet"], { cwd: root, stdio: "inherit" });
}

const left = problems(root, `v${version}`);
if (left.length > 0) {
  process.stderr.write(`Could not set every version:\n  ${left.join("\n  ")}\n`);
  process.exit(1);
}
process.stdout.write(`Set every version to ${version}.\n`);
