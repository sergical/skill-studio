# MCP-served skills plan

Skill Studio will explain what its inventory, model listing estimate, and
usage report cover. You will then be able to enter the name and address of
one running MCP server and see the skills it lists. A recorded pi session
will show whether Skill Studio can recognize an actual load in a later
usage update. [Issue #770](https://github.com/sergical/skill-studio/issues/770)

## Landing order

These are plan IDs. Replace each with its GitHub PR link when opened.

| Plan ID | Change | Branch from | Wait for | Files |
| --- | --- | --- | --- | --- |
| [PR 1](#pr-1--label-what-skill-studio-reads) | Label the scope of inventory, model listing, and usage | `main` | Backend merge queue below | Desktop report headers, CLI help and text, MCP tool descriptions, targeted tests and docs |
| [PR 2](#pr-2--list-one-server-from-the-cli) | List one explicitly chosen server from the CLI | `main` after PR 1 | PR 1 | Host catalog client, CLI route, protocol fixture, verification helper and flow docs |
| [PR 3](#pr-3--show-the-catalog-in-the-desktop-app) | Let a desktop user request the same catalog | `main` after PR 2 | PR 2 | Tauri command, Settings, frontend IPC, harness, desktop e2e and flow docs |
| [PR 4](#pr-4--capture-one-pi-session) | Capture a pi session with browse and load evidence | `main` after PR 2 | PR 2 for the fixture, PR 3 for landing order | Verification recipe, sanitized session fixture and capture check |

PR 3 and PR 4 can branch independently once PR 2 lands. PR 1 starts from
current `main`. Keep the fixes for [MCP launcher cache failure #759](https://github.com/sergical/skill-studio/issues/759)
and [Claude Code doctor listing budget #595](https://github.com/sergical/skill-studio/issues/595)
ahead of these merges. Use the updated budget behavior as the PR 1 baseline.

PR 2 and PR 3 require owner review of the new interaction before merge.
Show the exact command or control and the traffic it triggers. Apply the
repo's [review and risk rules](../AGENTS.md) to each implementation PR. If
PR 2 enables an SDK feature that triggers [`gate/risk`](../.gate/config.toml),
the owner merges it. Automatic connections, process launch from agent
config, and sign-in are future product decisions. The planned opt-in
prototype works with a running, unauthenticated loopback HTTP endpoint.

## Evidence for every PR

- Use [verify-skill-studio](../.agents/skills/verify-skill-studio/SKILL.md)
  to launch a disposable home, run `doctor`, and record the binary SHA,
  source commit, commands, results, and disk state. Use the real local
  `npx skills` pack to install `bench-skill-00` when a story needs a local
  skill. Use the wrapper for JSON. Run the same built binary with the same
  disposable home and minimal environment for help and human-readable text.
  Check the saved exit status for each CLI call.
- Run the desktop stories on WebKit and Chromium. The
  [desktop harness](../.agents/skills/verify-skill-studio/features/desktop-harness.md)
  proves the React interaction with a mocked backend. Run the live protocol
  or Tauri check specified below for the backend. Read the dev-servers
  skill before starting the harness or a fixture server. Stop the processes
  started for the run and run `control-skill-studio cleanup` after saving
  evidence under the run's `evidence/` directory.
- For performance, measure `main` first, then the candidate with the same
  build profile, machine, fixture, and warmed filesystem. Save 10 raw samples
  per timed operation, the median and p95, and the request or process counts.
  The thresholds below are proposed acceptance limits. Use the pinned
  toolchain and `--workspace --all-features` for Rust. Keep 30 GB free, run
  at most two Rust builds at once, and use the repo's check queue.

## PR 1 — Label what Skill Studio reads

- [ ] Dependencies. Take the post-budget-fix baseline described above.
- [ ] Files. Use the headers in [Skills](../apps/desktop/src/components/SkillList/SkillsView.tsx),
  [Model listing](../apps/desktop/src/components/Listing/ListingView.tsx)
  and [Activity](../apps/desktop/src/components/Activity/SkillActivityView.tsx),
  [CLI help](../apps/cli/src/main.rs) and [output](../apps/cli/src/output.rs),
  [MCP tool descriptions](../apps/mcp/src/lib.rs), [capabilities](capabilities.md),
  and the desktop e2e stories for these reports.
- [ ] Change. Describe inventory and recent-use counts as covering skills
  this computer can see on disk. Describe Model listing and Doctor's budget
  estimate as using local skills and available recorded evidence. The
  [listing estimator](../crates/skill-studio-core/src/listing_visibility.rs)
  can include entries from recorded sessions, such as bundled skills and
  MCP prompts. Say that Skill Studio does not query live MCP skill catalogs
  for these reports. Keep the machine-readable result contracts and the
  [scan](../crates/skill-studio-core/src/ops.rs) and
  [usage calculation](../crates/skill-studio-host/src/usage_report.rs) as they are.
- [ ] Seen. A user can tell what the scope applies to beside the report
  they are reading. The CLI help and MCP tool descriptions make the same
  distinction. Session transcripts are evidence for observed sessions.
- [ ] Unit. Run the existing installed-skill usage tally and recorded
  Codex system-skill listing regressions. Run the scan/listing/usage JSON
  contracts to catch a result-shape change.
- [ ] Live. In a verify run install one local skill and declare an MCP
  server in the disposable pi config whose command would write a marker
  if started. Use `scan`, `library`, `listing`, `doctor`, and
  `usage --harness pi` and inspect their scope text. Run the built
  [stdio MCP server](../apps/mcp/README.md#environment-variables) with
  `SKILL_STUDIO_HOME` set to that home. Call `scan`, `library`, `listing`,
  `doctor` and `skill_usage`, and check `tools/list`. Pass when the local
  skill is accounted for, scan and usage agree with trunk for the same input,
  the marker is absent, and the descriptions make the scope visible. In
  desktop e2e open Skills, Model
  listing and Activity using navigation. Check that the local entries
  and counts are under the scope shown to the user. Check the scope wording
  once in each report with the live output, without snapshotting prose.
- [ ] Perf. Time the built CLI's `scan`, `listing`, and `usage` on the
  same warmed verify home. Fail if any median increases by more than the
  larger of 10% or 20 ms, or if a read starts any MCP or `npx` process.

## PR 2 — List one server from the CLI

- [ ] Dependencies. PR 1 is merged. Read the
  [Skills overview](https://modelcontextprotocol.io/extensions/skills/overview),
  [beta wire spec](https://github.com/modelcontextprotocol/ext-skills/blob/main/specification/beta/skills.mdx),
  and [base protocol lifecycle](https://blog.modelcontextprotocol.io/posts/2026-07-28/)
  before choosing the transport. SEP-2640 is final and the extension is beta.
- [ ] Files. Add the protocol client in [skill-studio-host](../crates/skill-studio-host/src/)
  with a small CLI entry in [main.rs](../apps/cli/src/main.rs) and
  [output.rs](../apps/cli/src/output.rs). Touch the relevant Cargo manifests
  and lockfile only if wiring the existing transport requires it. Add the
  loopback fixture and live recipe under
  [verify-skill-studio](../.agents/skills/verify-skill-studio/), the CLI
  integration story under [CLI tests](../apps/cli/tests/), and update
  [flows](flows.md) and [capabilities](capabilities.md).
- [ ] Change. Add an explicit experimental command such as
  `skill-studio served-skills --server fixture --url http://127.0.0.1:PORT/mcp`.
  The user supplies the display name and the single URL. Validate a literal
  loopback address, and connect directly without redirects, ambient proxy
  settings, or credentials. Keep this transport in the host. The existing
  [extras reader](../crates/skill-studio-core/src/extras.rs) returns declaration
  names and paths, and desktop's
  [agent setup](../apps/desktop/src-tauri/src/skills/skill_mcp_connect.rs)
  installs Skill Studio as an outgoing server. Neither path is a catalog
  connection or a source of connection details for this prototype.
- [ ] Protocol. Use the locked `rmcp` 3.9.0 client's typed `list_skills`
  operation. Confirm its Streamable HTTP transport can enforce the direct
  connection and response bounds against the fixture before wiring the
  CLI. The wire sequence for 2026-07-28 is `server/discover`, then
  `skills/list` only if `capabilities.extensions` advertises
  `io.modelcontextprotocol/skills`. Use per-request metadata and HTTP
  headers. Follow opaque cursors inside a single 5-second deadline, with
  at most 10 pages and 1 MiB of response data across the request. On a cap,
  fail visibly and report the catalog as incomplete. Present names and
  descriptions as plain text, keyed by server and opaque skill URI. Keep
  manifests out of the display result. Keep this command separate from
  scan, listing, usage, lifecycle and Skill Studio's own MCP tools.
- [ ] Seen. The command shows the requested server name and its returned
  skills, “No skills returned”, “Server does not advertise MCP Skills”, or a
  bounded connection/protocol error. The spec permits empty and partial
  lists. The output describes what the server returned.
- [ ] Unit. Feed the catalog decoder the pinned spec's response shape, with
  frontmatter and a cursor. Check display fields and an opaque URI that
  doesn't use `skill://`. Check the normal absent-capability branch.
- [ ] Live. Use the fixture recipe below and run the built CLI via
  verify-skill-studio. Select the populated server, then change the fixture
  to empty and then stop it to check recovery from a connection failure.
  Include a server without the extension. Pass when the CLI reports the
  four distinct outcomes and the populated run records one discovery
  and two lists, with the second using the first page's cursor. Check zero
  list calls for the unsupported run and zero
  `skills/get`, `resources/read`, tool calls or `npx` calls. Before and
  after, compare the local skill tree, lock and journal state and scan
  result. The served entry appears only in the catalog result. Make the
  populated and empty CLI journey a runnable integration test against the
  same fixture process.
- [ ] Perf. Against trunk, record `scan`, `listing`, and `usage` medians
  with the fixture running and zero fixture requests. Fail the candidate if
  an ordinary report adds any requests or increases any median by more than
  the larger of 10% or 20 ms. For the new route, time two sequential raw
  fixture calls on the trunk setup as a reference. Fail if the candidate
  one-page request uses more than two RPCs or exceeds 500 ms median wall time
  on loopback. Verify a silent fixture returns a failure by 5 seconds plus
  500 ms of launch and measurement allowance.

## PR 3 — Show the catalog in the desktop app

- [ ] Dependencies. PR 2 and its verify fixture are merged. Use its host
  operation and error mapping.
- [ ] Files. Add the IPC command under
  [desktop skills](../apps/desktop/src-tauri/src/skills/), register it in
  [the Tauri entry point](../apps/desktop/src-tauri/src/lib.rs), and wire
  the [Settings](../apps/desktop/src/components/Settings/) panel and
  [dev harness](../apps/desktop/src/dev/harness/). Add a
  [desktop e2e](../apps/desktop/e2e/) story and a feature recipe under
  [verify-skill-studio](../.agents/skills/verify-skill-studio/features/).
- [ ] Change. Put a read-only catalog panel in Settings. Accept the server
  name and loopback URL for the current view and request them only when the
  user presses “List skills”. Display pending, populated, empty, unsupported
  and failed states for the requested server. Clear the previous result
  when a fresh request starts. Use the same restrictions as the CLI. Show
  name and description beside server attribution. Provide no local skill
  actions on served rows.
- [ ] Seen. Opening the panel is quiet. An explicit request shows one
  server's returned catalog. A failed request gives the user a way to retry.
  The regular Skills view continues to show the installed skill.
- [ ] Unit. Cover status mapping at the IPC boundary or the frontend's
  request-state logic at its existing seam. Use one offline-to-retry case
  that would otherwise leave the previous rows on screen.
- [ ] Live. In both harness browsers navigate by role, enter the endpoint,
  list the two fixture entries, and retry after a failed request. Before
  submission, prove the IPC handler was never called. After submission,
  check the server-labelled list and the local Skills view, and prove the
  story fails against pre-change UI. Run the same flow once in the built
  Tauri app against the real fixture. Set `SKILL_STUDIO_FIXTURE` to the
  verify home and use an untracked Tauri config override with a disposable
  bundle identifier. Verify the resolved data paths are isolated before
  driving the app. Use the PR 2 CLI command in the same
  verify run as the independent reference. Save the visible result and the
  fixture log. Pass when opening Settings yields zero requests, clicking
  yields one discovery and the expected list pages, and both surfaces show
  the same entries and outcomes.
- [ ] Perf. Measure opening Settings on trunk with the same harness home.
  Fail if the candidate starts any connection on open or raises median
  time to visible Settings controls by more than the larger of 10% or 30 ms.
  On the real app, fail a one-page request if it exceeds two RPCs or takes
  more than 750 ms median from click to visible rows on loopback.

## PR 4 — Capture one pi session

- [ ] Dependencies. Reuse PR 2's fixture. Read the published
  [pi extension package](https://pi.dev/packages/@jangmanj/pi-mcp-ext)
  and the source in the [published 0.2.0 tarball](https://registry.npmjs.org/@jangmanj/pi-mcp-ext/-/pi-mcp-ext-0.2.0.tgz).
  Start with [pi 1.5.2](https://registry.npmjs.org/@earendil-works%2fpi-coding-agent/1.5.2),
  the extension's declared minimum, and record the two versions used.
  Review the extension before running it in the disposable home.
- [ ] Files. Add a recipe and a small capture check under
  [verify-skill-studio](../.agents/skills/verify-skill-studio/) and save a
  sanitized transcript excerpt under [verification](verification/).
  Read the existing [pi parser](../crates/skill-studio-core/src/skill_uses/pi.rs)
  and [usage report](../crates/skill-studio-host/src/usage_report.rs) to
  record the import boundary. Keep this PR to evidence and its replay.
- [ ] Change. Record the real pi JSONL for browsing and then loading the
  same skill. The release source gives a candidate signal. It writes an
  approval entry before fetching and verifying bytes, and a successful
  `mcp_skills` load returns a framed skill and server/URI/name details.
  Confirm which of those fields pi actually persists. Count a load in the
  capture check only when a matching `mcp_skills` call with `action: "load"`
  has a successful result by tool-call ID, with matching server and URI.
  A plain resource read has the same transport as a load and cannot
  establish activation. If the saved transcript lacks the required
  evidence, record the gap and stop before proposing an importer.
- [ ] Seen. A reviewer can point to separate browse and successful load
  records in one session, or to the exact missing field that prevents
  attribution. Today's `usage` joins history to installed names. Any
  later import needs an explicit server/URI identity and must keep same-name
  served and local skills distinct before it can change a count.
- [ ] Unit. Replay the redacted real excerpt through the capture check.
  Assert zero loads for the browse portion and one for the full session.
  Removing the successful result should produce zero even with the
  approval record present.
- [ ] Live. In the verify home, configure one loopback server in
  `home/.pi/agent/mcp.json` under `mcpServers.fixture`. Run actual pi from
  an empty project inside the run, using an isolated model configuration
  and a test account or a local test model if available. Ensure pi writes to
  `home/.pi/agent/sessions`. Ask pi to list the fixture skills and stop,
  then in the same interactive session ask it to load the named skill
  and follow its harmless instruction. Approve pi's prompt for the fixture.
  A fresh unapproved load needs interactive approval in this extension.
  Pass when the session and wire log together show zero loads during browsing
  and one successful approved load with the expected bytes and identity.
  Save the raw isolated capture in the run evidence and commit only the
  sanitized excerpt. Run `usage --harness pi` and `scan` from the built
  CLI with and without the captured session. Use a same-name installed
  skill as a control for misattribution and record what trunk reports
  before designing any import.
- [ ] Perf. Establish a trunk `usage --harness pi` baseline with the same
  captured JSONL, then repeat on the candidate. Fail for any MCP request
  made by `usage` or a median increase above the larger of 10% or 20 ms.
  For the capture itself compare its wire counts with a browse-only run
  of the same pinned pi setup. Expect zero skill-body reads in browse and
  exactly one for a fresh approved load of the single-file fixture.

## One fixture for catalog and activation

Create a small process in PR 2 that binds an ephemeral `127.0.0.1` port,
prints the endpoint, and logs method, cursor and resource URI to the verify
run. Use the existing local pack for the independent installed-skill control.
The MCP process serves two synthetic skills in memory under distinct names,
including `skill://served-check/SKILL.md`. It can return two pages, an empty
listing, or discovery without the extension. Implement `server/discover`
and `skills/list` with the beta response shape, including `resultType`,
`ttlMs` and `cacheScope` where required. Provide `skills/get` and
`resources/read` for pi, using the same UTF-8 bytes to compute manifest
size and SHA-256. For example, make the single-file skill ask for the
response “fixture loaded”. Put its body only behind `resources/read`.
Save the fixture PID and wire log at launch and stop that process on cleanup.
The runner needs no GitHub fixture repository or MCP account.

## What remains open

The plan has source-level evidence for the transport and pi load signal.
Wire interoperability, real Tauri behavior, persisted pi fields and
performance numbers are measured by the PR checkboxes. The loopback
prototype does not establish compatibility with hosted, authenticated,
stdio or legacy servers. Activation capture supports a later usage design
decision. Lifecycle actions for served skills and publishing the local
library through Skill Studio's MCP server remain outside this issue.
