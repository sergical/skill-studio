# Merge gate

The gate records intent, classifies a committed diff, and collects proof on the
head commit. It uses Python 3.11+ and `gh`, with no added package dependency.
Python is already used by this repo's performance scripts. Run commands with
`scripts/gate/gate`, or add that directory to PATH.

## Design and scope

All repo facts live in `.gate/config.toml`. Python code has no project paths.
Rules set a minimum tier; an operator can only raise it. Contract snapshots use
command output and JSON data, so edits to CLI or MCP implementation files alone
do not stop a PR. Keep the first release in three stacked PRs: risk and contracts;
intent and proof; workflows and metrics. Each part has a separate review.

Snapshot checks run the configured executables. Build them first with
`cargo build --workspace --all-features --bins`, then run `gate snapshots`.
Use `gate snapshots --update` to accept a contract change. Review the snapshot
diff. CLI help includes visible commands recursively and the configured hidden
commands. MCP snapshots contain tool names and input schemas. Schema snapshots
contain the committed JSON schemas. A stale snapshot fails the check.

`gate risk --json` prints machine data. Without that flag it prints the tier and
reasons. `--base` defaults to the configured base; the comparison uses the merge
base. `--head` allows a historical commit. Renames include both old and new paths.
`--raise stop --reason "..."` records a higher tier for this invocation.
New manifest dependencies stop. Dependabot patch-only changes ship unless an
independent rule stops them. Lockfiles remain covered by normal CI.

## Adopt in a TypeScript web app

```sh
# Copy scripts/gate/ and the two gate workflows into your repo.
mkdir -p .gate
cat > .gate/config.toml <<'TOML'
repository = "owner/web-app"
base = "origin/main"
check = "npm test"
dependency_files = ["package.json"]
version_files = ["package.json"]
ci_jobs = ["test"]
rules = [{ id = "gate", tier = "stop", paths = [".gate/**", "scripts/gate/**", ".github/**"] }]
TOML
scripts/gate/gate risk --json
```

No Rust code or Rust contracts are required. Add `[[contracts]]` with `kind =
"cli"` and a command array for help output, or `kind = "files"`, `glob`, and
`snapshot` for JSON contracts. Each contract needs a committed snapshot path.

## Tests

Run `python3 -m unittest discover -s scripts/gate -p 'test_*.py'`.
Tests create temporary git repositories and do not call a live account.

## Intent and review

`gate start "Users can ..." --author-model <model>` saves branch intent under
Git's private directory. `gate start --issue <HTTPS URL>` can link an issue.
Author model is optional. If both model names are known, a matching reviewer
is refused. Use exact provider model IDs, not aliases.

Write findings as `{"head":"<full SHA>","findings":[{"severity":"high",
"resolved":false}]}`. Run `gate review-status --verdict pass --reviewer <model>
--findings findings.json`. Any unresolved high or critical finding posts failure.
A deferred finding remains unresolved. Unknown authors are permitted and are not
proof of model independence. Statuses belong to the head SHA; pushes need a new
review. Run review-status only after the review is complete.

`gate card --result "One-line result"` replaces only its marked block in the PR
body. It reads checks from GitHub and `.gate/out/verify.json`. Missing evidence
shows `verify: not run`. Local evidence paths are listed as local. Upload them
and use HTTPS paths to show images to GitHub reviewers. Never use machine paths
or private details in an evidence manifest intended for a public PR.

## Approval and agent guard

`gate:approved` is a human approval convention. Agents use the owner's token, so
this label is not a technical boundary. Configure each agent's pre-tool-use hook
to run `scripts/gate/gate guard` with the tool event JSON on stdin. Exit 2 blocks
commands containing the approval label, including URL-encoded colons. For Claude
Code, put this hook in local settings, not shared account settings:

```json
{
  "hooks": {
    "PreToolUse": [
      { "matcher": "Bash", "hooks": [{ "type": "command", "command": "scripts/gate/gate guard" }] }
    ]
  }
}
```

This guard must be installed in the agent host before agent use. The gate does
not claim to sandbox a token or prevent an agent from bypassing its own hooks.
No approval label is added by any gate command.
