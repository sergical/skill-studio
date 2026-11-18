# Merge gate

The gate classifies a committed diff, checks contract snapshots, and records
the review for the head commit. It uses Python 3.11+ and `gh`, with no added package dependency.
Python is already used by this repo's performance scripts. Workflow shell fixtures also use the existing `jq` tool, which GitHub-hosted runners provide. Run commands with
`scripts/gate/gate`, or add that directory to PATH.

## Design and scope

All repo facts live in `.gate/config.toml`. Python code has no project paths.
Rules set a minimum tier; an operator can only raise it. Contract snapshots use
command output and JSON data, so edits to CLI or MCP implementation files alone
do not stop a PR.

Tiers: **ship** and **show** PRs merge by auto-merge when CI and the review
pass. A **stop** PR gets the `risk:owner` label, its `gate/risk` status stays
pending, and the owner merges it.

Each stop rule has an `[owner.<rule id>]` table with `ask` (the decision the
owner makes) and `why` (why a person must make it). `gate risk --json` returns
them in `needs_you`, one entry per rule with the files that matched. A stop rule
without this text still stops, and its `why` says the text is missing.

Snapshot checks run the configured executables. Build them first with
`cargo build --workspace --all-features --bins`, then run `gate snapshots`.
Use `gate snapshots --update` to accept a contract change. Review the snapshot
diff. CLI help includes visible commands recursively and the configured hidden
commands. MCP snapshots contain tool names and input schemas. Schema snapshots
contain the committed JSON schemas. A stale snapshot fails the check.

`gate risk --json` prints machine data. Without that flag it prints the tier and
reasons. `--base` defaults to the configured base; the comparison uses the merge
base. `--head` allows a historical commit. Renames include both old and new paths.
`--raise stop --reason "..."` prints a higher tier for this invocation.
New manifest dependencies stop. Dependabot patch-only changes ship unless an
independent rule stops them. Lockfiles remain covered by normal CI.

## Adopt in a TypeScript web app

```sh
# Copy scripts/gate/ and the gate workflows into your repo.
mkdir -p .gate
cat > .gate/config.toml <<'TOML'
repository = "owner/web-app"
base = "origin/main"
dependency_files = ["package.json"]
version_files = ["package.json"]
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

## Workflows and metrics

`gate-risk.yml` runs the `risk` job on `pull_request_target`, only for PRs to the default branch. GitHub uses base-branch YAML for this event. The job checks out the base SHA without stored credentials. It fetches the PR head as git objects and runs only base-branch scripts. For a stop tier it adds the `risk:owner` label, posts one PR comment with a `Needs you: <ask>` line, the `why` and the files for each rule, and posts `gate/risk = pending` with the first `ask`. Otherwise it removes that label and that comment and posts `gate/risk = success`. It never posts failure, so the risk check sends no failure email.

`gate.yml` runs `unit` and `contracts` on `pull_request` with read access only. These checks run PR code. They do not control the risk job. Both are required checks in the ruleset.

Do not execute PR code in a workflow that has write access. The owner merges a stop PR with the ruleset bypass. Agents push with the owner's account, so the label and the pending status stop agent mistakes, not an agent that uses `--admin`.

`gate-update.yml` updates behind PRs that have auto-merge enabled. Supply an
optional `GATE_UPDATE_TOKEN` secret with contents and pull-request write access
so update pushes trigger CI. GitHub suppresses most workflows from pushes made
with the default workflow token. Without that secret, updates can occur but a
maintainer must trigger checks. Conflicts or head changes fail the job and leave
the PR for a maintainer. Agents enable auto-merge when they open a PR.

`gate-metrics.yml` runs on day 1 of each month and on manual dispatch. It reports the previous calendar month in the Actions job summary. It has read access to contents, pull requests, Actions runs, and commit statuses. It does not write to the repository.

`gate metrics --since 2026-08-01 --until 2026-11-14 --last 250` prints outcomes
and an optional backtest. Only PRs merged into the repository's default branch are selected; the branch name comes from GitHub repository metadata. It paginates GitHub API data and caches immutable PR
merge facts in `.gate/out/history`. Delete that cache to refresh it. Rates use
PRs as the denominator; a PR can match several rules. Behind means the main
parent immediately before the merge was absent from the PR head. Review proof
must be success before the merge timestamp. Follow-up fixes use titles and
shared files or PR references within 14 days; this is a heuristic. Recent PRs
have an incomplete observation period. Historical CLI and MCP snapshots do not
exist, so historical contract stop counts are a lower bound. No code can recover
those outputs from file paths alone. Reports state this limitation.

The privileged risk job checks out base-branch gate code and policy, so PR
changes cannot change their own classification. A change to `gate-risk.yml`
runs only after it merges. Unit and contract jobs run PR code with read-only
permissions. The private-repo fetch uses a one-time auth header.

Main CI metrics count push runs from this repository and the configured `ci_workflows` only. Historical merge ancestry supports squash and merge commits; rebase-merge history needs a separate base reconstruction and is not supported. `--last` refuses a window with too few PRs; widen `--since`. Main-commit and PR follow-up windows both extend 14 days past the reporting period, and merged PR commits are counted once.
