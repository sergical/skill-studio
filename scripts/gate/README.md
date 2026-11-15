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

`gate:approved` is a human approval trigger. The approval workflow records `gate/approved = success` on the SHA captured by the label event, then removes the label. Risk reads only the approval status on its exact head SHA. A new push has no approval status, even when an old label event runs at the same time. Agents use the owner's token, so
this label is not a technical boundary. Configure each agent's pre-tool-use hook
to run `scripts/gate/gate guard` with the tool event JSON on stdin. Exit 2 blocks
commands containing the approval label or `gate/approved` status, including URL-encoded separators. It also blocks merges past required checks: `gh pr merge --admin`, the REST merge endpoint and the GraphQL `mergePullRequest` mutation. A plain `gh pr merge` stays allowed, because it refuses a blocked PR. For Claude
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

This guard must be installed in the agent host before agent use. The gate does
not claim to sandbox a token or prevent an agent from bypassing its own hooks.
No approval label or approval status is added by any gate command.

The guard is a best-effort string match, not a token sandbox. It blocks quoted, escaped and URL-encoded approval labels and fails closed on malformed input, even outside a git checkout.

Verify manifests keep the documented scenarios format. An optional `head` field binds evidence to a commit. A mismatched head shows not run; evidence without a head is rendered as unbound and cannot claim a verified current head.

## Workflows and metrics

`gate.yml` posts risk for each PR head and reruns for label removal or addition. The proof card lists `gate/approved` separately from risk and review. Without a passing approval status on the current SHA, a stop-tier PR fails risk.
Fork PR tokens cannot write statuses. A maintainer must run classification from
a trusted checkout and post its result; fork runs fail closed. Do not use
`pull_request_target` to execute untrusted PR code with a write token.

`gate-bypass.yml` runs on each push to `main`. When the push is a PR merge whose head did not pass every check that the branch rules require, `gate bypass --sha <sha>` comments on the PR with the account, the time and each red or missing check. GitHub records only the account for a merge, so with the guard hook in every agent, a bypass record points to a human or to an agent without the hook.

`gate-update.yml` updates behind PRs that have auto-merge enabled. Supply an
optional `GATE_UPDATE_TOKEN` secret with contents and pull-request write access
so update pushes trigger CI. GitHub suppresses most workflows from pushes made
with the default workflow token. Without that secret, updates can occur but a
maintainer must trigger checks. Conflicts or head changes fail the job and leave
the PR for a maintainer. This workflow does not enable auto-merge.

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
cannot change their own classification. The first gate PR needs a manual
bootstrap because base does not yet contain the gate. Unit and contract jobs
run PR code with read-only permissions.

The approval workflow uses `pull_request_target: labeled` with no checkout or PR code execution. It runs independently of the risk workflow, so risk concurrency cannot cancel it. Labels are never read or removed by the risk job. Automatic branch updates produce a new SHA and need fresh human approval. Risk waits for unit and contract checks; a stale contract therefore fails gate/risk even when no stop rule matched. The private-repo fetch uses a one-time auth header.

Main CI metrics count push runs from this repository and the configured `ci_workflows` only. Historical merge ancestry supports squash and merge commits; rebase-merge history needs a separate base reconstruction and is not supported. `--last` refuses a window with too few PRs; widen `--since`. Main-commit and PR follow-up windows both extend 14 days past the reporting period, and merged PR commits are counted once.

The approval workflow is installed on the default branch. Its first installation needs a human bootstrap approval; agents must not post `gate/approved` themselves. A failed removal leaves only a trigger label, which risk never accepts as approval.
