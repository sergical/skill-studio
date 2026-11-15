# Merge gate

The gate records intent, classifies a committed diff, and collects proof on the
head commit. It uses Python 3.11+ and `gh`, with no added package dependency.
Python is already used by this repo's performance scripts. Workflow shell fixtures also use the existing `jq` tool, which GitHub-hosted runners provide. Run commands with
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
# Copy scripts/gate/ and the gate workflows into your repo.
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

`gate:approved` is a human approval trigger. The approval workflow records `gate/approved = success` on the SHA captured by the label event, then removes the label. Risk reads the status list for its exact head SHA, newest first. It ignores entries unless `creator.login` is `github-actions[bot]`. Each candidate must link to an Actions run in this repository. The run must use `.github/workflows/gate-approve.yml` and event `pull_request_target`, and its run name must be `gate:approved <head SHA>`. Each PR of the run must have the default branch as its base, and the run must start after the last base change of the PR. A branch that an agent makes can hold its own approval workflow, so an approval from another base does not count, also after the PR moves to the default branch. Risk runs again when the base changes. The approval workflow runs only for PRs to the default branch. The approval workflow sets this run name from the label event, and `pull_request_target` always runs the base-branch workflow, so a PR cannot change it. Risk removes any `@ref` suffix from the run path before it checks the path. The newest bot entry that passes these checks decides approval. Its state must be `success`. A new push has no approval status, even when an old label event runs at the same time. Agents use the owner's token, so
this label is not a technical boundary.

A workflow on any branch also posts as `github-actions[bot]`, and the status
writer chooses the run URL. The run checks stop a copied URL: a run of another
workflow, of another event, or for another commit does not count. A copied URL
of a real approval run for the same commit adds nothing, because that commit is
already approved.

Configure each agent's pre-tool-use hook
to run `scripts/gate/gate guard` with the tool event JSON on stdin. Exit 2 blocks
commands containing the approval label or `gate/approved` status, including URL-encoded separators. For Claude
Code, put this hook in local settings, not shared account settings:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": ".*",
        "hooks": [
          { "type": "command", "command": "\"$CLAUDE_PROJECT_DIR\"/scripts/gate/gate guard" }
        ]
      }
    ]
  }
}
```

The hook runs for every tool. The guard skips only named file tools, such as `Read`, `Write`, `Edit`, `MultiEdit`, `NotebookEdit`, and `apply_patch`. Agents can edit code and docs. The guard checks shell tools, `Monitor`, GitHub tools, unknown tools, and events without a tool name.

This guard must be installed in the agent host before agent use. The gate does
not claim to sandbox a token or prevent an agent from bypassing its own hooks.
No approval label or approval status is added by any gate command.

The guard is a best-effort string match, not a token sandbox. It blocks quoted, escaped and URL-encoded approval labels and fails closed on malformed input, even outside a git checkout.

Verify manifests keep the documented scenarios format. An optional `head` field binds evidence to a commit. A mismatched head shows not run; evidence without a head is rendered as unbound and cannot claim a verified current head.

## Workflows and metrics

`gate-risk.yml` runs the `risk` job on `pull_request_target`. GitHub uses base-branch YAML for this event. The job checks out the base SHA without stored credentials. It fetches the PR head as git objects and runs only base-branch scripts. It posts `gate/risk` for that head and runs again when a human changes a label. It has read access to Actions runs to check the source of approval.

`gate.yml` runs `unit` and `contracts` on `pull_request` with read access only. These checks run PR code. They do not control the risk job. After merge, the maintainer must add `unit` and `contracts` to the ruleset as required checks. A passing `gate/risk` alone does not show that these checks passed.

The proof card lists `gate/approved` separately from risk and review. Without a trusted passing approval status on the current SHA, a stop-tier PR fails risk. Do not execute PR code in a workflow that has write access.

`gate-update.yml` updates behind PRs that have auto-merge enabled. Supply an
optional `GATE_UPDATE_TOKEN` secret with contents and pull-request write access
so update pushes trigger CI. GitHub suppresses most workflows from pushes made
with the default workflow token. Without that secret, updates can occur but a
maintainer must trigger checks. Conflicts or head changes fail the job and leave
the PR for a maintainer. This workflow does not enable auto-merge.

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

The privileged risk job checks out base-branch gate code and policy. PR changes
cannot change their own classification. If the base commit has no approval helper, risk treats approval as absent.
This keeps stop-tier changes blocked during rollout. The move to
`gate-risk.yml` also needs a maintainer rollout. The new workflow must be on
the base branch before it can run. This PR cannot post its own trusted risk
status before that installation. The first gate PR needs
a manual bootstrap because base does not yet contain the gate. Unit and contract jobs
run PR code with read-only permissions.

The approval workflow uses `pull_request_target: labeled` with no checkout or PR code execution. Each status links to its source run through `target_url`. It runs independently of the risk workflow, so risk concurrency cannot cancel it. Labels are never read or removed by the risk job. Automatic branch updates produce a new SHA and need fresh human approval. The private-repo fetch uses a one-time auth header.

The approval and risk jobs can start at the same time. Risk can read statuses
before approval posts its status. So the approval workflow removes the label
with `GATE_UPDATE_TOKEN`, and the `unlabeled` event runs risk again after the
approval. Without that secret it uses the Actions token, which starts no
workflow. Then a maintainer changes a label after approval to run risk again.

Main CI metrics count push runs from this repository and the configured `ci_workflows` only. Historical merge ancestry supports squash and merge commits; rebase-merge history needs a separate base reconstruction and is not supported. `--last` refuses a window with too few PRs; widen `--since`. Main-commit and PR follow-up windows both extend 14 days past the reporting period, and merged PR commits are counted once.

The approval workflow is installed on the default branch. Its first installation needs a human bootstrap approval; agents must not post `gate/approved` themselves. A failed removal leaves only a trigger label, which risk never accepts as approval.
