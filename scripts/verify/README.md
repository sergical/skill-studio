# Verify Skill Studio

Build once, then run the actual CLI and its stdio MCP server:

```sh
cargo build --workspace --all-features
scripts/verify/run.py --cli-only
```

The runner uses Python's standard library. Each scenario gets a fresh temporary
HOME and an allowlisted environment. Codex, XDG, dotagents, cwd, and temporary
files stay inside it. Installer shims are fixtures, so no network is needed.
It never copies user skill libraries or credentials. Homes are removed even on
failure. `.gate/out/verify.json` and relative evidence files remain for review.
A failed assertion or timeout records `fail` and makes the runner exit nonzero.
This runner does not create `scripts/gate/` or gate configuration.

The explicit `--cli-only` mode is the release smoke subset. A full verification
requires both `--rust-artifacts` and `--ui-url`; missing inputs record blocked
scenarios as `fail`, so a partial run cannot appear complete.

For desktop command regressions, compile existing regression tests and pass the
Cargo artifact stream. Compilation uses the normal toolchain home; the compiled
test processes run inside the disposable home, without invoking Cargo:

```sh
mkdir -p .gate/out
cargo test --workspace --all-features --no-run --message-format=json > .gate/out/artifacts.jsonl
```

For rendered checks, use the existing Vite dev harness. Start it in a Rex terminal
through portless as required by AGENTS.md, then pass its URL:

```sh
scripts/verify/run.py --rust-artifacts .gate/out/artifacts.jsonl --ui-url https://skill-studio-verify.localhost
```

Install the repo's Playwright Chromium runtime first if absent. Set
`PLAYWRIGHT_BROWSERS_PATH` when it is outside the default macOS cache. The browser
uses a disposable profile. The dev harness mocks Tauri IPC; these screenshots
prove available controls, not native installer behavior or Finder launch PATH.
The native command runner test covers PATH resolution. A packaged `.app` needs a
separate release smoke check on macOS.

The built process scenarios cover all six agent roots and the shared library,
CLI/MCP agreement, split → undo → undo of undo, and a symlinked dotagents shim
with restored ownership, lock, and ignore files. The current CLI batch Park path
restores manifest entries semantically; it can change their order. Optional native regressions
cover configured install roots, packaged command PATH, nested copy leases, and
a real second process holding a lease. Optional rendered checks cover the hidden
Pack method and removed Settings health cards. These follow #305, #364/#365,
#414, #293, #331, and #304 respectively.

Use `--binary` to exercise another built binary or an installed npm launcher.
Historical revisions can lack today's commands or tests. Record that as a limit;
do not count a missing command, zero matching tests, or a build failure as proof
that the historical bug was reproduced. The agent adds 1 to 3 intent scenarios
and records observations as described in the repository skill.

The HOME guard reads the OS account record with `getent` on Linux and the shell's
account lookup on macOS. It compares raw paths when the account home is absent.
Tests require the explicit home-identity refusal, rather than accepting an
unrelated panic. Every CLI/MCP test launcher clears inherited configuration;
explicit fixture overrides remain available for environment contract tests.

The health-card check first rejects historical plain-div labels before checking
the real Settings page. The copy contention scenario runs real CLI split-copy
and undo commands against a lease held by the test process, checks files and
history while busy, and retries after release. Existing native desktop tests
continue to cover the independent-copy command's nested lease behavior.
