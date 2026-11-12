# Audit: agent design, API design, system design

Date: 2026-11-11. Base commit: `9364e639`.
Status: complete. Four scans (proxy, agent surface, CLI and core, desktop)
checked against the source. Every finding is kept. The backlog at the end
lists all fixes; finding ids (P, D, U, T, A, V, C, W) point to the evidence.

## Sources

| Id  | Source                                                                            |
| --- | --------------------------------------------------------------------------------- |
| R   | rubriclabs.com/blog/everything-we-know-about-good-agent-design (25 rules, R1–R25) |
| H   | rubriclabs.com/blog/what-is-an-agent-harness                                      |
| C   | rubriclabs.com/blog/how-does-claude-code-actually-work                            |
| P   | rubriclabs.com/blog/primitives-over-pipelines                                     |
| U   | rubriclabs.com/blog/unblocking-agents                                             |
| D   | raindrop.ai/blog/agent-self-diagnostics                                           |
| A   | seangoedecke.com/good-api-design                                                  |
| S   | seangoedecke.com/good-system-design                                               |
| —   | openrouter.ai (no rule for us), openai.com/index/asana (HTTP 403, not read)       |

Verdicts: ✅ match, 🟡 partial, ❌ gap, n/a does not apply (Skill Studio calls no LLM).

## Summary

### R — agent design

| Rule                                     | Verdict | Evidence                                                                                                                                                                                                                   |
| ---------------------------------------- | ------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| R1–R2 read prompt, keep short            | ✅      | MCP `instructions` are fixed, 52 words (`apps/mcp/src/lib.rs:935`). AGENTS.md is about 2.6k words.                                                                                                                         |
| R3–R4 old models, model selector         | n/a     | No LLM calls. `tiktoken` only counts listing cost.                                                                                                                                                                         |
| R5 tools around use cases                | ✅      | Tidy-up: `diagnose` → `skill_usage` → `park`/`remove` → `restore_event`.                                                                                                                                                   |
| R6 smallest tool set                     | 🟡      | 25 tools listed by default; 4 dev tools hidden for context cost (`lib.rs:95`).                                                                                                                                             |
| R7 no overlap                            | 🟡      | `fix` vs `preview_`/`apply_frontmatter_repair`; `update` vs `update_all`; `turn_off_check` vs `turn_off_for_agent(confirmed=false)`; undo split between `restore_event` and `unpark`.                                      |
| R8 no missing capability                 | ✅      | `outdated` states that it needs `gh` and reports `unknown` without it.                                                                                                                                                     |
| R9 literal names                         | 🟡      | Vague: `doctor`, `extras`, `invocation`, `fix`, `outdated`. Word order mixed: `review_set` vs `list_events`.                                                                                                               |
| R10 permissions in tools                 | ❌      | Refusals come back with a reason ✅, but the fixture sandbox does not block writes (C54, C55), there is no global dry run (C51), and MCP read-only hints are not enforced (C56).                                           |
| R11 errors as feedback                   | ✅      | Example: "More than one copy can be changed. Pass one of these ids: …".                                                                                                                                                    |
| R12 split expected and unexpected errors | 🟡      | Exit 2/3/4 split (`crates/skill-studio-core/src/error.rs:55`), but `io` and `execution_failed` share exit 2 with caller errors (C62); no not-found or internal code (C63, C64); desktop drops the code (W rules, R11/R12). |
| R13 agent knows only its context         | ✅      | `scan`: "Call this first." `doctor` measures the cost of each harness's skill index.                                                                                                                                       |
| R14 environment visibility               | 🟡      | Envelope has `scope`, `schema_version`, `correlation_id`; `capabilities` and `harnesses` hidden by default.                                                                                                                |
| R15–R16 compaction                       | n/a     | Harness concern.                                                                                                                                                                                                           |
| R17 skills for procedure                 | 🟡      | Workflow only in server instructions; no SKILL.md teaches agents to use Skill Studio; `fix` and `turn_off_for_agent` bundle steps.                                                                                         |
| R18–R21 subagents                        | n/a     | Not in the product. Own process: the T3 rule "every new review is a new `delegate_task`" conflicts with R20 "reuse subagents".                                                                                             |
| R22 guardrails outside agent             | ✅      | Core enforces confinement, lease and hash check; `trust_confirmed` for unknown dotagents sources.                                                                                                                          |
| R23 trusted vs untrusted data            | ❌      | SKILL.md bodies, descriptions and frontmatter returned unlabelled (`crates/skill-studio-core/src/dto.rs:370`, U1–U36). Desktop loads remote images from SKILL.md and has `csp: null`.                                      |
| R24 log everything                       | 🟡      | Journal per write; one Sentry transaction per op. Reads and refusals not logged.                                                                                                                                           |
| R25 self-report tool                     | ❌      | None.                                                                                                                                                                                                                      |

### Linked articles

| Source                               | Verdict | Evidence                                                                                                                                                                                    |
| ------------------------------------ | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| P primitives, thin prompt            | ✅/🟡   | "One tool per Operation"; preview/apply split; `fix` and `turn_off_for_agent` are small pipelines.                                                                                          |
| C fail loudly, tool cost             | ✅      | `stale_proposal`, `ambiguous_target`; dev tools hidden for context cost.                                                                                                                    |
| H CLI equals MCP                     | ✅      | CLI `--json` prints the same envelope as MCP.                                                                                                                                               |
| A do not break userspace, pagination | 🟡      | `schema_version` in every envelope ✅; but no envelope schema, goldens only for scan and diagnose (C4, C6), no CHANGELOG (C12); `list_events` has `limit`/`after` but no next cursor (C25). |
| U sandbox                            | 🟡      | `SKILL_STUDIO_FIXTURE` exists; `apps/mcp/README.md:80` claims "read-only" but no code blocks writes.                                                                                        |
| D self-diagnostics                   | ❌      | Same as R25.                                                                                                                                                                                |

### S — system design

| Rule                        | Verdict | Evidence                                                                                                                                                             |
| --------------------------- | ------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Simple first                | ✅      | AGENTS.md "Before you implement an issue" (500-line stop). One core for CLI, MCP and desktop.                                                                        |
| One owner writes each state | 🟡      | ✅ for external files ("the tool that owns a file writes it"). ❌ inside the app: see below.                                                                         |
| Schema and indexes          | ✅      | `idx_events_skill (skill, ts DESC)`, `idx_events_ts` (`crates/skill-studio-host/src/history.rs:103`).                                                                |
| Slow work in background     | ✅      | Pool of 4 for outdated lookups (`crates/skill-studio-core/src/skill_update_check.rs:280`); scan returns `incomplete` past its budget.                                |
| Cache little                | ✅      | Core caches live for one call (`CachedSkillRead`, `LookupCache`).                                                                                                    |
| Events not overused         | ✅      | Events are an undo journal, not a bus.                                                                                                                               |
| Push vs pull                | ✅      | File watcher (`apps/desktop/src-tauri/src/skills/skill_refresh.rs:21`).                                                                                              |
| Hot paths                   | ✅      | `scan` measured in `docs/perf/` with p95.                                                                                                                            |
| Log unhappy paths           | 🟡      | Two `eprintln!` in core; no MCP logs; Sentry gets error code only.                                                                                                   |
| p95/p99                     | ✅      | `docs/perf/README.md:17`.                                                                                                                                            |
| Killswitches                | ❌      | Only telemetry has a switch. No switch for desktop auto-update, the proxy URL, or features (P killswitch, desktop S killswitches).                                   |
| Retries, idempotency        | 🟡      | Caller decides on `scope_busy`; recovery converges ✅. But repeat restore, park, add fail (C15–C18); a stuck recovery plan blocks all writes with no way out (C107). |
| Fail open / closed          | ✅      | Reads return partial; writes refuse on drift; telemetry off without consent or DSN.                                                                                  |
| Boring components           | 🟡      | Hand-written confinement with a check-then-use race (C113–C115); unbounded backups (C38); four hand-written atomic writes in the desktop.                            |

### Two writers for the same state

- Desktop `event_store.rs` has its own `CREATE TABLE` for the events database; host says "kept in sync by hand" (`crates/skill-studio-host/src/history.rs:84`).
- Update check twice: desktop 4,175 lines, core 1,392 lines.
- `skill_independent_copy.rs` (2,397 lines) and `skill_materialize.rs` (1,492 lines) do not call the core; core has `ops_split.rs`.

### Doc drift

- `apps/mcp/README.md:19` lists 7 tools; 25 are listed.
- `packages/marketing/src/docs/content/mcp.md:85` says per-agent turn-off "is coming"; `turn_off_for_agent` ships.
- `docs/spec-headless-performance-observability.md` says `skill-studio mcp serve`; the command is `skill-studio mcp`.

## Proxy server (`apps/server`) and its clients

Files: `proxy-app.ts` = `apps/server/src/skills-proxy-app.ts`; `api.rs` =
`apps/desktop/src-tauri/src/skills/api.rs`; `counts.rs` =
`apps/desktop/src-tauri/src/skills/skill_install_counts.rs`.

The proxy only reads (GET only) and its input checks are careful. The weak
parts are error text, retries, and README claims.

| Rule                        | Verdict | Evidence                                                                                                                                                         |
| --------------------------- | ------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A do not break userspace    | 🟡      | Proxy has no version of its own; passes upstream `/api/v1` bodies through (`proxy-app.ts:17`, `:373`). Client needs fixed fields (`api.rs:104-109`, `:150-157`). |
| A simple auth               | ✅      | Key is a Worker secret (`worker.ts:28`); desktop sends no auth to the proxy (`api.rs:209`).                                                                      |
| A idempotency               | n/a     | GET only; other methods get 405 (`proxy-app.ts:311-316`).                                                                                                        |
| A rate limits               | 🟡      | 60 per 60 s per IP (`wrangler.jsonc:13-21`); 429 sends only `Retry-After: 60` (`proxy-app.ts:325-327`).                                                          |
| A pagination                | 🟡      | Page numbers with `hasMore` (`api.rs:131-135`); no cursor; search has no paging.                                                                                 |
| A expensive fields optional | ❌      | Detail returns every file's contents (`api.rs:156`); client keeps one file (`api.rs:508`).                                                                       |
| S stateless                 | ✅      | App built per request (`worker.ts:73`).                                                                                                                          |
| S caching                   | ✅      | Edge cache 300 s / 3600 s, 200 only (`proxy-app.ts:226-228`, `:345`).                                                                                            |
| S slow work in background   | ✅      | Install counts: 3 at once, 250 ms apart (`counts.rs:27-28`).                                                                                                     |
| S log unhappy paths         | 🟡      | Upstream 5xx, non-JSON, redirect, network, unhandled reported. Upstream 401/429 and own 400/429 only in the log line.                                            |
| S metrics                   | 🟡      | Every request traced; `duration_ms` logged; no p95/p99 or alerts in repo.                                                                                        |
| S killswitch                | ❌      | None; release URL compiled in (`api.rs:27-28`).                                                                                                                  |
| S retries                   | 🟡      | No retries anywhere; only install counts pause after 429.                                                                                                        |
| S fail open/closed          | 🟡      | Browse and detail show retry buttons; counts fall back to cache; "repo public" check fails closed.                                                               |
| S boring components         | ✅      | Hono, Workers rate limit, Workers Cache, reqwest.                                                                                                                |
| R11 actionable errors       | 🟡      | See P52, P53.                                                                                                                                                    |
| R12 expected vs unexpected  | 🟡      | Proxy splits `upstream` vs `unhandled`; client errors are plain strings; Sentry span gets ok/not-ok only.                                                        |
| R23 untrusted content       | ✅      | `skill_md` shown as plain text in `<pre>` (`SkillContent.tsx:32-34`).                                                                                            |
| R24 log everything          | 🟡      | Every proxy request logged; `get_install_counts` not timed (`commands.rs:121-146`).                                                                              |

### All proxy findings

Contract and shapes:

- P1 `proxy-app.ts:17`, `:205-210` — fixed upstream base, same `/api/v1` paths, no own version, no shape translation.
- P2 `proxy-app.ts:373`, `:385`, `:404` — upstream status and body passed through; upstream shape changes reach clients.
- P3 `api.rs:104-109` — `SkillV1` needs `id`, `name`, `installs: u32`, `source`; a removed or renamed field fails the whole list.
- P4 `api.rs:150-157` — detail needs `hash` and `files`; missing either gives "Failed to parse skill details" (`:226`).
- P5 `api.rs:362` documents `domain.com/slug` ids; proxy accepts only 3 segments (`proxy-app.ts:157`, `segments.length === 7`) and returns 400 "Invalid skill detail path".
- P6 `SkillDetailPanel.tsx:47` — loads when `split("/").length >= 3`; 4+ segment ids pass but the proxy rejects them.
- P25 `api.rs:101-102`, `:116` — list items have no `description`; client sets `None`.

Auth and keys:

- P7 `server.ts:26-31` — Node dev server refuses to start without `SKILLS_SH_API_KEY`, never logs it.
- P8 `worker.ts:23-28` — production key is a Worker secret; deploy workflow never sees it.
- P9 `proxy-app.ts:54`, `worker.ts:108` — key only in `Authorization`; `tracePropagationTargets: []` keeps trace headers off upstream.
- P10 `api.rs:209-213` — developer key marked sensitive but stored as plain text in `~/.agents/skill-studio.json` (`skill_fork_registry.rs:200-203`).
- P11 `api.rs:209`, `proxy-app.ts:322` — no client auth to the proxy; access control is per-IP rate limit only.
- P12 `proxy-app.ts:311-316` — non-GET gets 405.
- P70 `server.ts:23-24`, `api.rs:211` — key never logged; header error formats the error, not the key.

Rate limits and paging:

- P13 `wrangler.jsonc:13-21` — one limiter for all routes; the heavy detail route has no tighter limit.
- P14 `proxy-app.ts:325-327` — 429 has no `X-RateLimit-*` headers.
- P15 `proxy-app.ts:322` — key `CF-Connecting-IP ?? "unknown"`; no allowlist.
- P16 `proxy-app.ts:320-344` — limiter runs before cache; cache hits use quota (test `skills-proxy-app.test.ts:260`).
- P17 `proxy-app.ts:321`, `server.ts:38` — Node dev server has no limiter.
- P18 `api.rs:489-490`, `:453-468` — only detail reads `Retry-After`; search and list show generic "status: 429" (`:304-306`, `:346-348`).
- P19 `api.rs:470` — default 429 pause is 15 min; proxy says 60 s.
- P20 `api.rs:325-332`, `proxy-app.ts:236` — page numbers from 0, `hasMore`; no cursor.
- P21 `SkillStore.tsx:247-256` — Load more `page + 1`, `LIMIT = 50`, popular only.
- P22 `api.rs:275-277`, `SkillStore.tsx:222-229` — search not paged; `has_more` always false.
- P23 `proxy-app.ts:251-253` — params filtered by name only; `limit`, `per_page`, `page`, `view` values not range-checked.
- P24 `api.rs:156`, `:508`, `counts.rs:136` — detail returns all file contents; counts use only `installs`; no way to drop heavy fields.

State and caching:

- P26 `worker.ts:71-82` — app rebuilt per request; state only in edge cache and rate-limit binding.
- P27 `proxy-app.ts:226-228` — detail cached 3600 s; content and `hash` can be 1 h old.
- P28 `proxy-app.ts:345` — only 200 cached (tests `:276`, `:291`).
- P29 `proxy-app.ts:337-338` — cache key = origin + path + filtered sorted query; no purge.
- P30 `counts.rs:22`, `:32` — counts on disk 24 h; failures in memory 1 h.
- P31 `counts.rs:108-110` — repo-public result never cached.
- P32 `counts.rs:27-28`, `:325-331`, `:36` — 3 at once, 250 ms hold, 8 s timeout.

Logging and metrics:

- P33 `proxy-app.ts:76-78` — only upstream ≥ 500 reported; upstream 401 (bad server key) not reported, no expired-key alert.
- P34 `proxy-app.ts:66-73` — non-JSON reported only below 500 (no double report).
- P35 `proxy-app.ts:299-300`, `:392`, `:407`, `:325` — own 400s and 429 not reported.
- P36 `server.ts:38` — Node dev server has no `reportServerError`.
- P37 `proxy-app.ts:280-284`, `:274-277` — unhandled errors reported and return 500; `HTTPException` not reported.
- P38 `proxy-app.ts:292-294` — log line `{event, method, route, status, duration_ms}`; route template only.
- P39 `worker.ts:98` — `tracesSampleRate: 1`; no p95/p99 dashboards or alerts in repo.
- P40 `wrangler.jsonc:10-12` — Workers observability on.
- P56 `timing_log.rs:298-304`, `:311-319` — desktop sends Sentry ok/not-ok only; error text only in local `timing.jsonl`.
- P57 `commands.rs:121-146` — `get_install_counts` not wrapped in `time_command_async`.

Killswitch, retries, failure modes:

- P41 `worker.ts:27-31` — no killswitch or remote flag.
- P42 `api.rs:27-28`, `.github/workflows/release.yml:38-53` — URL compiled in; change needs a release or local `server_url`.
- P43 `proxy-app.ts:19`, `:50-51`, `:93` — 8 s upstream timeout covers the body; no retry.
- P44 `api.rs:196`, `:216` — client timeout also 8 s; client can give up before it sees the proxy's 502 "skills.sh request timed out".
- P45 `api.rs` — no retry or backoff on search, list, detail; manual retry buttons only.
- P46 `counts.rs:409-427` — 429 stops all lookups; no breaker for repeated 5xx or timeouts.
- P47 `counts.rs:116-121`, `:367-370` — GitHub 403/429 pauses all GitHub checks.
- P48 `browse-initial-load.ts:34` — installed and popular load in one `Promise.all`; a popular failure stops `setInstalledSkills` (`SkillStore.tsx:153`).
- P49 `SkillStore.tsx:92-103` — proxy down shows "Browsing isn't available right now" + message + Try again.
- P50 `SkillStore.tsx:259-264` — failed Load more shows a toast.
- P51 `useSkillInstalls.ts:20-23` — `.catch(() => {})` swallows count errors; backend never errors (`commands.rs:118-120`).

Error text:

- P52 `api.rs:186-191` — 401 says "skills.sh API key is invalid or expired" in server mode too, where the user has no key.
- P53 `api.rs:304-306`, `:346-348`, `:488-494` — client never reads error bodies; proxy text ("skills.sh request timed out", "Too many requests", "Invalid skill detail path") never reaches the user.
- P54 `proxy-app.ts:85-89` — network failure returns raw `error.message` to the caller.
- P55 `api.rs:241`, `:226` — direct mode and parse errors include reqwest error text.

Untrusted data and input checks:

- P58 `SkillContent.tsx:32-34` — `skill_md` in `<pre>`; no `dangerouslySetInnerHTML`.
- P59 `counts.rs:85-88`, `:51-60` — only safe `owner/repo` sources, after GitHub confirms public.
- P60 `proxy-app.ts:103-173`, `:391-393`, `server.ts:44-48` — layered path checks; three routes only.
- P61 `api.rs:370-376` — `get_skill_details` rejects empty, leading `/`, `..`, whitespace, but not `?` or `#`; per-segment builder (`:387-411`) used only for counts.
- P62 `api.rs:283-284`, `:326` — query URL-encoded; `limit`, `per_page` unchecked.
- P63 — no CORS headers (client is reqwest, not a browser).
- P64 — no cap on upstream response size.
- P65 `proxy-app.ts:56-62` — `redirect: "manual"`; 3xx becomes reported 502.

Sentry scrubbing:

- P66 `worker.ts:99-105` — `dataCollection` drops user, cookies, headers, bodies, query.
- P67 `sentry-event-scrub.ts:17-35` — events lose user, breadcrumbs, tz, headers, cookies, query; other contexts, tags, extra kept; owner/repo/slug paths stay in messages.
- P68 `sentry-event-scrub.ts:40-56`, `worker.test.ts:110` — spans lose UA, tz/locale, body sizes; span URL query removal relies on SDK.
- P69 `worker.ts:51-68` — incoming `sentry-trace`/`baggage` stripped; spans named by route template.

README and comment drift (verified):

- P71 `apps/server/README.md:20` "query string passed through verbatim" — code allowlists and sorts (`proxy-app.ts:235-257`).
- P72 `apps/server/README.md:41` "tracing stays off" — `tracesSampleRate: 1` (`worker.ts:98`).
- P73 `apps/server/README.md:57-58` "keyed by the full request URL" — key is filtered, sorted query (`proxy-app.ts:337-338`).
- P74 `skill_fork_registry.rs:206` default `http://127.0.0.1:8787` — dev builds only; release uses compiled URL.
- P75 `wrangler.jsonc:7-9` — `account_id` committed (identifier, not secret); `workers_dev: true` keeps `*.workers.dev` live beside `api.useskillstudio.com`.

## Agent surface, agent instructions, and docs

Files: `lib.rs` = `apps/mcp/src/lib.rs`; `dto.rs` =
`crates/skill-studio-core/src/dto.rs`; `main.rs` = `apps/cli/src/main.rs`;
`mcp.md` and `cli.md` are in `packages/marketing/src/docs/content/`.

Earlier claims checked again, all still true: 29 tools (`lib.rs:319-824`), 25
listed by default, 52-word instructions, no untrusted label, no feedback tool,
MCP README lists 7 tools, `mcp.md:85` "is coming", spec says `mcp serve`.

### Doc drift (D)

Gaps:

- D2 `apps/mcp/README.md:13` "One tool per core operation, named from `Operation`" — 5 exceptions: `review_set`/`review_clear` → `Review`, `turn_off_check` → `TurnOffForAgent`, `fix` → `FixSkill`, `add` → `Install`.
- D3 `apps/mcp/README.md:19-27` — 7 tools listed; 29 exist.
- D4 `apps/mcp/README.md:29-31` — says 3 tools open history; every write tool does.
- D5 `apps/mcp/README.md:47-63` — env table misses `SKILL_STUDIO_MCP_DEV_TOOLS` (`lib.rs:107`), `SKILL_STUDIO_TELEMETRY` (`lib.rs:161`), `XDG_DATA_HOME` (`apps/mcp/src/scope.rs:100`).
- D6 `apps/mcp/README.md:57` — fixture lease root `.history/leases`; code `.skill-studio/leases` (`apps/mcp/src/scope.rs:28`). Verified.
- D12–D13 `docs/spec-headless-performance-observability.md:124`, `:145` — `mcp serve`; command is `mcp` (`main.rs:365`).
- D15 `docs/spec-core-primitives.md:360-381` §8.2 — "tool names equal Operation names"; same 5 exceptions as D2.
- D19 `mcp.md:85` — per-agent turn-off "is coming"; ships (`lib.rs:715`, `:730`, `:750`).
- D23 `cli.md:95` — same; `turn-off --check/--confirm` ships (`main.rs:174-189`). Verified.
- D38 `apps/cli/README.md:11-23` — `--help` list has 9 commands; 22 are visible (`main.rs:95-503`).
- D41 `apps/cli/README.md:65` — fixture lease root wrong (`apps/cli/src/scope.rs:99`).
- D42 `apps/cli/README.md:66-70` — `--home` lease root wrong (`apps/cli/src/scope.rs:42-45`, `:114`).
- D43 `apps/cli/README.md:72` — `TranscriptProjectDiscovery` does not exist; code uses `HostProjectDiscovery` (`lib.rs:205`).
- D45 `apps/cli/README.md:92` — exit-2 list misses `backup_expired` (`crates/skill-studio-core/src/error.rs:63`).
- D47 `packages/npm/skill-studio/README.md:26` — says undo "skips" park and reverses the change before it; undo refuses with `unsupported` "Use unpark" (`apps/cli/tests/undo_fidelity.rs:63-92`). Verified.
- D50 `apps/mcp/tests/mcp_server.rs:1220` — dev-tool check lists `doctor` (not a dev tool) and omits `install_preferences`. Verified.

Partial:

- D1 `apps/mcp/README.md:8` and D17 `docs/spec-core-primitives.md:363` "no cache between calls" / "no state" — `lib.rs:125` keeps a usage index in memory.
- D8 `apps/mcp/README.md:71` `/path/to/skill-studio-mcp` — public docs use `npx skill-studio mcp` (`mcp.md:5`, `:10`).
- D9 `apps/mcp/README.md:80-82` "read-only sandboxed session" — fixture scope allows every write tool.
- D10 `apps/mcp/README.md:86-94` — lists 6 tested behaviours; about 32 tests exist.
- D14 `docs/spec-headless-performance-observability.md:124` — initial command list stale (now `preview-repair`, `apply-repair`, `events`, 22+ commands).
- D16 spec §8.2 — confirms only 7 tools.
- D18 `docs/spec-core-primitives.md:57` (D7) rejects `Result<T, String>`; AGENTS.md "Error Handling" prescribes it; desktop uses it (`commands.rs:65-109`).
- D20 `mcp.md:79-87` — capability list omits `doctor`, `extras`, `invocation`, review dates, `split`, turn-off-for-agent.
- D24 `cli.md:99-102` and D34 `README.md:71` "Undo the last change" — refuses when the newest event is a park or unpark.
- D25 `cli.md:150` and D44 `apps/cli/README.md:90-91` — exit-1 lists miss `doctor` (and `fix`, `conflicts` in the CLI README) (`apps/cli/src/ops.rs:288`, `:303`, `:308`).
- D26 `cli.md:134` — remove owners omit Copy and Fork (`lib.rs:633`, `ops_remove.rs:7`, `:46`).
- D28 `cli.md:83-86` "Turning a skill off means parking it" — per-agent turn-off now exists.
- D29 `get-started.md:56` — installs also go through dotagents (`ops_install_cli.rs:405`).
- D31 `README.md:59-72` — command table misses `invocation`, `review`, `turn-off`, `enable`/`disable`, `conflicts`, `extras`, `doctor`, `events`.
- D32 `README.md:65` "`fix` Repair broken frontmatter" — also repairs links and reports what it cannot fix.
- D37 `README.md:134` "Cloudflare Worker" — `apps/server` is also a Node dev server.

Match: D7, D11, D21, D22, D27, D30, D33, D35, D36, D39, D40, D46, D48, D49.

### Untrusted text returned to agents (U)

Rule R23. Third-party text returned as is, with no label. Only `extras` says
"may be … untrusted". Rows are in `dto.rs` unless named.

| Id      | Field                                                       | Line                           | Source                           |
| ------- | ----------------------------------------------------------- | ------------------------------ | -------------------------------- |
| U1      | `InstalledSkillDto.name`                                    | `:254`                         | frontmatter or folder name       |
| U2      | `InstalledSkillDto.description`                             | `:256`                         | frontmatter (author prompt text) |
| U3      | `DeploymentDto.path`                                        | `:163`                         | folder names                     |
| U4–U5   | `link_target`, `resolved_path`                              | `:171`, `:181`                 | symlink target                   |
| U6      | `symlink_error`                                             | `:185`                         | OS error with path               |
| U7      | `spec_violations`                                           | `:197`                         | may echo frontmatter             |
| U8      | `PluginSourceDto` marketplace, plugin, version              | `:137-141`                     | plugin manifest                  |
| U9      | `frontmatter` (incl. arbitrary `metadata`, `allowed_tools`) | `:201`, `frontmatter.rs:19-45` | SKILL.md                         |
| U10     | `frontmatter_fields`                                        | `:203`                         | raw SKILL.md values              |
| U11–U12 | `Issue.skill`/`message`, `Observation.message`              | `:95`, `:99`, `:120`           | names, paths                     |
| U13     | repair preview `path`, `reason`                             | `:357`, `:361`                 | SKILL.md                         |
| U14     | **`original_content` — full SKILL.md body (highest risk)**  | `:371`                         | SKILL.md                         |
| U15     | `proposed_content`, `diff`                                  | `:373`, `:375`                 | SKILL.md                         |
| U16     | `managed_update_warning`                                    | `:380`                         | path                             |
| U17     | `UnrepairedIssue`                                           | `:457-459`                     | paths, values                    |
| U18     | `ConflictSummary`                                           | `:469-475`                     | names, paths                     |
| U19     | `DoctorViolation`                                           | `:524-526`                     | frontmatter                      |
| U20     | `EventDto` skill, project_path, backup_dir                  | `:606-620`                     | recorded names                   |
| U21     | `CommandHealth.last_error`                                  | `:676`                         | child process                    |
| U22     | `ParkOutcome` parked_path, warnings                         | `:755-759`                     | git output                       |
| U23–U24 | `UnparkOutcome`, `SplitOutcome`                             | `:798`, `:830-839`             | paths                            |
| U25     | `AgentOffRefusal.reason`                                    | `:877`                         | path, name                       |
| U26–U27 | `RemoveOutcome.skill`, `UpdateOutcome.deployment_path`      | `:1083`, `:1164`               | name, path                       |
| U28     | `UpdateAllOutcome.errors`                                   | `:1191`                        | npx stderr                       |
| U29     | remove error with raw npx stderr                            | `ops_remove.rs:341`            | child process                    |
| U30     | `ErrorEntry` message, path                                  | every envelope                 | any of the above                 |
| U31     | `ExtraEntry` (partial: only labelled one)                   | `extras.rs:17-33`              | third-party agent config         |
| U32–U34 | `OutdatedRecord`, `CommitInfo`, `OutdatedTarget`            | `skill_update_check.rs:58-140` | GitHub, `gh`, lock file          |
| U35     | `ListingSkillCost.name`                                     | `listing_cost.rs:18`           | SKILL.md                         |
| U36     | `add` NeedsTrust identity                                   | `lib.rs:648`                   | dotagents source                 |

Not checked: `SkillUsage`, `review_due` output, `InvocationReport`,
harness `--version` strings, install stdout/stderr.

### Tool descriptions (T), all 29

| Tool                         | Line           | Finding                                                                                                                                                                                 |
| ---------------------------- | -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `scan`                       | `:319`         | no read/write statement; `timings` "per-phase timings" is jargon                                                                                                                        |
| `diagnose`                   | `:334`         | no return shape; read only in annotation only; overlaps `doctor`, `diagnose_conflict`                                                                                                   |
| `review_set`                 | `:349`         | maps to `Review`; no return statement; `date` optional in schema but required at runtime (`:359`)                                                                                       |
| `review_clear`               | `:369`         | `date` published but ignored (`req.date = None`, `:378`). Verified                                                                                                                      |
| `review_due`                 | `:384`         | `skills`, `timings` published but ignored (`_req`, `:390`)                                                                                                                              |
| `skill_usage`                | `:399`         | no read/write statement; keeps a cache (`:125`)                                                                                                                                         |
| `capabilities` (dev)         | `:429`         | `tools` param: "Needs the `ToolLookup` port" jargon; overlaps `harnesses`                                                                                                               |
| `harnesses` (dev)            | `:444`         | no read/write statement                                                                                                                                                                 |
| `preview_frontmatter_repair` | `:459`         | CLI name is `preview-repair`                                                                                                                                                            |
| `apply_frontmatter_repair`   | `:477`         | CLI name `apply-repair`; overlaps `fix`                                                                                                                                                 |
| `list_events`                | `:495`         | return and read/write not checked                                                                                                                                                       |
| `restore_event`              | `:510`         | does not say a park returns `unsupported`                                                                                                                                               |
| `fix`                        | `:525`         | name not literal; maps to `FixSkill`; overlaps apply repair                                                                                                                             |
| `diagnose_conflict`          | `:540`         | CLI name `conflicts`; overlaps `diagnose`                                                                                                                                               |
| `extras`                     | `:555`         | name does not say hooks, MCP servers, instruction files                                                                                                                                 |
| `doctor`                     | `:570`         | name not literal; no read/write statement; overlaps `diagnose`                                                                                                                          |
| `invocation`                 | `:585`         | reads or writes by input, annotated readOnly=false; `deployment_id` "Desktop edits one exact copy." jargon                                                                              |
| `update` / `update_all`      | `:603`, `:618` | overlap; "See [`InstallLinkMode`]", "Request wrapper for `ops::update_all` … only exists so an MCP tool has one schema" jargon                                                          |
| `remove`                     | `:633`         | "canonical shared .agents/skills copy owned by skills.sh, dotagents, Copy or Fork" jargon                                                                                               |
| `add`                        | `:648`         | maps to `Install`; `trust_confirmed` step not explained                                                                                                                                 |
| `install_preferences` (dev)  | `:663`         | not checked                                                                                                                                                                             |
| `park` / `unpark`            | `:678`, `:765` | ✅ params documented                                                                                                                                                                    |
| `turn_off_check`             | `:715`         | reuses `TurnOffForAgent`; overlaps `turn_off_for_agent`                                                                                                                                 |
| `turn_off_for_agent`         | `:730`         | params have no field docs, inherit type jargon ("Opaque deployment id. Invariant: … `dep:v1/`", "serde wire name used by the desktop app today"); no `deny_unknown_fields` (`:110-117`) |
| `split`                      | `:750`         | name not literal; overlaps `turn_off_for_agent`                                                                                                                                         |
| `outdated`                   | `:799`         | no read/write statement                                                                                                                                                                 |
| `sweep_quarantine` (dev)     | `:824`         | ✅ "cannot be undone", destructive                                                                                                                                                      |

Cross-cutting: every core schema field has a description; empty ones are only
local MCP structs. No test snapshots `tools/list` or checks descriptions;
`tools_list_publishes_hints_and_an_undo_for_every_write`
(`apps/mcp/tests/mcp_server.rs:1155`) only checks for the word "undo".

### Repo agent instructions (A): AGENTS.md

AGENTS.md: 484 lines, 2,578 words, about 3.4k tokens; `CLAUDE.md` links to it.
No `.claude/` or `.agents/` folder in the repo.

Gaps:

- A5–A7 Build commands contradict each other: `cd apps/desktop/src-tauri && cargo …` (Rust Commands), "Rust: `cargo test` in `apps/desktop/src-tauri`" (Testing), and `check` without `--all-features` (`package.json:27`) versus "Always pass `--workspace --all-features`".
- A8 "React 19.1, TypeScript 5.8" — `react ^19.3.1`, `typescript ^7.0.2` (`apps/desktop/package.json:29`, `:56`). Verified.
- A11 "package.json # npm workspaces root" — repo uses `pnpm-workspace.yaml`.
- A12 import example `./lib/skill-types` — file is `packages/lib/src/skill-types.ts`.
- A13 `discover_all` example — not in `apps/desktop/src-tauri/src`.
- A15 Error Handling snippet shows `Option`, not `Result`.
- A17 "Invalidate caches by setting `_cachedSections: null`" — not in the code. Verified.
- A18–A20 Key Files `apps/desktop/src/lib/skill-types.ts`, `skills/scan.rs`, `skills/provenance.rs` — missing. Verified.
- A30 (R17) procedures are prose in AGENTS.md (mutation testing, worktree and disk rules, "Before you implement an issue"), not skills.
- A31 (R13, R17) `.cursor/skills/verify-skill-studio` is the only repo skill, Cursor-only, never named in AGENTS.md.

Partial:

- A2 "36,000+ skills" — cannot be checked.
- A3 "via `npx skills`" — dotagents too; Copy/Fork via quarantine.
- A4 provenance list misses project repo and Copy/Fork owners.
- A10, A37 `apps/server` is a Worker plus a Node dev server.
- A14 `Result<T, String>` rule contradicts spec D7.
- A22 "colocated tests" — `apps/desktop/src-tauri/tests/` has 10 integration files.
- A23 (R13) Reference docs miss `docs/spec-*.md`, `docs/perf/README.md`, `docs/verification/*`, `apps/mcp/README.md`, `apps/cli/README.md`, the verify skill.
- A26 `npx skills add/remove/update` — dotagents omitted.
- A28 agent table misses cross-harness reads (OpenCode reads `.claude/skills`, Cursor reads `.claude` and `.codex`, Grok reads `.claude`; `harness.rs:617-624`, `:829`, `:904`), plugin caches, Codex `.codex/skills` "inferred" (`:555`).
- A29 "see agents.rs for AgentId" — canonical id is the core newtype (`identity.rs:15-31`, `open-code`).
- A32 (R1, R2) 3.4k tokens per session, mostly desktop/frontend detail, no MCP or CLI guidance.

Match: A1, A9, A16, A21, A24, A25, A27.

### Verification coverage (V)

- V1 ✅ `docs/flows.md:11-58` checked by `apps/cli/tests/flow_parity.rs:399-437`; 18 flows covered.
- V2 ❌ `docs/flows.md:31-58` — about 25 flows state no CLI/MCP test (project folders, discovery sources, telemetry, plugin enable, repair link, discard parked copy, skills.sh search/details/popular, app update, read/write SKILL.md, fork, pull upstream, editor choice, plugin update/uninstall, materialize, independent copy, check updates now, browse details, undo split update).
- V3 ❌ Popular skills: Test "none" (`flows.md:48`).
- V4 ❌ No flows.md row: `diagnose`, `conflicts`, `capabilities`, `install_preferences`, `sweep_quarantine`, `undo`, `watch`, `health`, `schema`.
- V6 ❌ `flow_parity` does not require a row per tool or command.
- V7 🟡 parity runs with dev tools on (`apps/cli/tests/support/headless.rs:98`), not the default surface.
- V8 ✅ stdio tests spawn the real binary (about 32 tests).
- V9 ❌ no `tools/list` snapshot.
- V10 ❌ `install_preferences` never called in MCP tests.
- V11 ❌ `sweep_quarantine` never called.
- V12 ❌ no CLI test for `review_clear`.
- V13 ❌ dev-tool hiding test lists `doctor` (same as D50).
- V14 🟡 desktop checks exist (Playwright `verify-home.spec.ts`, `docs/verification/*`, `apps/desktop/src-tauri/tests/park_parity.rs`), not linked from AGENTS.md.
- V15 ❌ no end-to-end test of the instructions workflow (`diagnose` → `skill_usage` → `park` → `restore_event`/`unpark`).
- V5, V16 ✅.

Not checked: `.github/workflows/*` for agent runs;
`docs/agent-skill-conventions.md`, `docs/state-machines.md`,
`docs/verification/*.md` bodies; the verify skill's contents.

## CLI and core contract

Files: `ops.rs`, `error.rs`, `journal.rs`, `dto.rs`, `ports.rs` are in
`crates/skill-studio-core/src/`; `main.rs` = `apps/cli/src/main.rs`.

| Area                             | Verdict | Findings  |
| -------------------------------- | ------- | --------- |
| A do not break userspace         | 🟡      | C1–C13    |
| A idempotency                    | 🟡      | C14–C23   |
| A pagination                     | 🟡      | C24–C28   |
| A expensive fields optional      | 🟡      | C29–C33   |
| A bounds on bulk ops             | ❌      | C34–C39   |
| R6 smallest tool set             | 🟡      | C40–C43   |
| R9 literal, consistent names     | 🟡      | C44–C50   |
| R10 permissions, dry run         | ❌      | C51–C60   |
| R11/R12 errors, exit codes       | 🟡      | C61–C77   |
| R14 environment visibility       | 🟡      | C78–C83   |
| R24 logging                      | 🟡      | C84–C94   |
| P primitives                     | 🟡      | C95–C99   |
| S fail open reads, closed writes | 🟡      | C100–C104 |
| S retries in recovery            | 🟡      | C105–C112 |
| S boring components              | 🟡      | C113–C118 |

Userspace:

- C1 `crates/skill-studio-core/src/lib.rs:95` `SCHEMA_VERSION = 1`; rises only when a field is removed or changes meaning (`:92-94`).
- C2 `ops.rs:362`, `:413` — every envelope has `schema_version`.
- C3 `error.rs:17-18`, test `:207-246` — error names and exit statuses never change.
- C4 `apps/cli/tests/envelope.rs:205`, `:246` — envelope goldens cover only `scan` and `diagnose`; no golden for any write.
- C5 `envelope.rs:919-955` — snapshot test guards the 47 schema files.
- C6 no schema for `ResultEnvelope` or `ErrorEntry`; the envelope is not snapshot-guarded (`apps/cli/src/output.rs:747-888`).
- C7 ✅ aliases kept: `--deployment-id`, `--harness`, `history`, `enable`/`disable` (`main.rs:201-528`).
- C9 `main.rs:669` — `add --global` accepted and ignored.
- C10 `main.rs:2181`, `:2218` — `health` (always exit 0) and `watch` lines have no envelope or `schema_version`.
- C11 ✅ no BREAKING or `!:` commits in the last 200.
- C12 no CHANGELOG.
- C13 no deprecation warnings; aliases are silent.

Idempotency:

- C14 ✅ `ops.rs:4235` — repeat apply-repair returns `AlreadyApplied`.
- C15 `ops.rs:4647`, `:5212`, `:5205-5208` — repeat restore is an error (`already_reverted`, exit 3) and first writes a Failed row.
- C16, C71 already parked: by name `InvalidRequest` (`main.rs:1797`), by id `Unsupported` (`ops.rs:6741-6758`). One condition, two codes.
- C17 `ops.rs:5738-5845` — repeat batch park fails `Unsupported`.
- C18 `crates/skill-studio-core/src/ops_install.rs:484-492` — repeat add fails `InvalidRequest`.
- C19 `crates/skill-studio-core/src/ops_invocation.rs:178-206` — every invocation set writes a backup and events, even when the policy is already set.
- C20 ✅ `crates/skill-studio-core/src/review.rs:54-94` — review set/clear overwrite.
- C21 no idempotency key or request id.
- C22 `ops.rs:292-300` — `fix` envelope `event_id` is only the last repair's id.
- C23 `ops_invocation.rs:62` — `InvocationReport` has an empty `impl Outcome`; envelope `event_id` is None though the report holds `event_ids`.

Pagination:

- C24 `dto.rs:626-638`, `ops.rs:55` — `list_events` has `limit` (default 200) and `after`.
- C25 `ops.rs:4338-4426` — returns a bare list; no next cursor, no `has_more`.
- C26 `main.rs:1628-1702` — `undo` pages history itself.
- C27 `dto.rs:287-292` — no limit on `scan`, `diagnose`, `usage`, `outdated`.
- C28 ✅ `ports.rs:645-660` — doctor history reads at most 256 rows.

Expensive fields:

- C29–C30 `main.rs:116`, `ops.rs:386-423` — `--timings` hidden, and envelope `timings` are always filled anyway.
- C31 ✅ `check_drift` opt-in (`dto.rs:626-638`).
- C32 ✅ `capabilities --observe` opt-in.
- C33 `dto.rs:509` `DoctorRequest {}` — doctor always runs every check plus the listing budget.

Bulk bounds:

- C34 `crates/skill-studio-core/src/ops_update.rs:1352-1368` — `update_all` has no size bound; one lease per item.
- C35 `ops.rs:5676-5736` — batch park/unpark: no bound, stops at the first failure, no rollback ("can leave earlier items parked").
- C36 no rate limit or throttle.
- C37 `ops.rs:459-466` — `scan_for_usage` uses `Duration::MAX` budget.
- C38 `ops.rs:59-64`, `journal.rs:43`, `:53` — caps 2 MiB SKILL.md, 2000 files, 64 MiB folder, 16 MiB journal; **`MAX_BACKUP_BYTES = u64::MAX`** (unbounded). Verified.
- C39 `main.rs:2218` — `watch` polls every 200 ms, no end.

Tool set and names:

- C40 CLI: 22 visible commands (`main.rs:97-374`), 10 hidden (`:382-492`).
- C41 MCP: 29 tools, 14 read, 15 write, 4 dev-gated.
- C42 Overlaps: `enable`=`unpark`, `disable`=`park`; `events`/`history`/`list_events`; MCP `update`+`update_all` vs CLI `update`; MCP `turn_off_check`+`turn_off_for_agent` vs CLI `turn-off --check|--confirm`; `invocation` checks and sets; `fix` vs preview/apply repair. CLI only: `undo`, `watch`, `health`, `schema`, `mcp`.
- C43 `split` is a public MCP tool but hidden in the CLI (`main.rs:382-394`).
- C44 `crates/skill-studio-core/src/ops_discard.rs:32-37` — one op, three names: `remove`, `discard`/`DiscardOutcome`, `Operation::Remove`.
- C45 `--agent` (add, turn-off) vs `--harness` (usage, split, capabilities).
- C46 three on/off vocabularies: park/unpark, disable/enable, turn-off.
- C47 id flags: `--id`, `--deployment-id`, `--event-id`.
- C48 `--method` has a default on add, a string parser on update.
- C49 skill is `--skill` on fix and update, positional on park and unpark.
- C50 `main.rs:1499` — single update builds its runtime with `UpdateAllOutcome`.

Permissions and dry run:

- C51 no global `--dry-run`.
- C52 ✅ previews: `turn-off --check`, `preview-repair` (hidden), `invocation` with no flag.
- C53 ✅ `main.rs:2385-2386` — `turn-off` without `--confirm` refused.
- C54 **`SKILL_STUDIO_FIXTURE` does not block writes**; `apps/mcp/README.md:80-82` says "read-only sandboxed session" (`apps/mcp/src/scope.rs:21-30`, `lib.rs:201-216` only turn discovery off).
- C55 CLI `--fixture` the same (`main.rs:846`, `:888`).
- C56 MCP read-only and destructive hints are not enforced.
- C57 dev tools gated by `SKILL_STUDIO_MCP_DEV_TOOLS=1`.
- C58 `main.rs:2062-2065` — `usage` writes its own CLI cache file. (Scan said it breaks "never writes"; checked: that comment is about the desktop cache, so it is correct. The read still writes a file.)
- C59 `main.rs:1581`, `:2090` — `events` and `outdated` build the write runtime for a read.
- C60 `review.rs:54-94` — review writes no history event; cannot be undone.

Errors and exit codes:

- C61 ✅ 14 codes; exits 2, 3, 4, 130 (`error.rs:21-74`).
- C62 `error.rs:55`, `:64-65` — `ExecutionFailed` and `Io` exit 2 like caller errors.
- C63 no `not_found` code; uses `AmbiguousTarget` (`ports.rs:1596-1612`, `ops.rs:3974`, `:5584`, `:6789`) or `InvalidRequest` (`main.rs:1777`, `ops.rs:4639`).
- C64 no internal-error code; `journal.rs:710-713` reports an invariant as `InvalidRequest` "invalid recovery settlement".
- C65 `ops.rs:434` — an error with no entries exits 1, same as "ok with issues".
- C66 `ops.rs:322-336`, `dto.rs:1186-1196` — `update_all` errors are `BTreeMap<String,String>` without codes; partial run says "some roots were not read; see observations" (`ops.rs:399-402`); all-failed exits 1 with empty `errors`. Verified type.
- C67 `ops.rs:1836-1844` — several batch name failures collapse into one `InvalidRequest`.
- C68 `error.rs:103-108`, `:182-189` — `LeaseBusy{pid, age}` does not reach the caller.
- C69 `error.rs:162` — Io message is only the OS text.
- C70 `main.rs:909-942` — CLI `error_envelope` uses the raw path, skips `sanitized()`.
- C72 `journal.rs:722-741` — write blocked by recovery reports `incomplete` (exit 4), whose meaning is "roots not read in time". Overloaded.
- C73 vague messages: `ops.rs:4639` "event not found" (no id), `:4654`/`:4672` "this event cannot be restored", `:4678` "restore_capability() reported Yes but the event has no inverse" (internal text), `:4737` "not implemented", `:4509`/`:4513` "restore target has no parent directory / file name", `:5738-5845` "Name is repeated." / "No skill with this name was found." (no name), `:5993`, `:6273`, `:6282` (no path); `ops_invocation.rs:74`, `:91`, `:195`; `ports.rs:88-115` "path lies outside the scope"; `ports.rs:651-659` "doctor history unavailable".
- C74 ✅ exits 1, 2, 3, 4 tested (`envelope.rs:244-553`).
- C75 `packages/npm/skill-studio/bin/skill-studio.js:12-22` — missing platform package exits 1 (clashes with "ok with issues").
- C76 `apps/cli/src/output.rs:29-34` — JSON serialization failure exits 70, not in the table.
- C77 `main.rs:2306`, `:2327` — CLI validation errors skip `rt.run`, so telemetry misses them.

Environment visibility:

- C78 envelope: schema_version, operation, scope (id, kind, home, projects, history_root), status, data, errors, correlation_id, event_id, timings.
- C79 no binary version, commit, or platform in the envelope.
- C80 `capabilities`/`harnesses` hidden in CLI, dev-gated in MCP.
- C81 `apps/cli/src/scope.rs:67-72` — under `--home`/`--fixture` the usage cache is None; not checked whether the envelope says so.
- C82 ✅ data root `$XDG_DATA_HOME/skill-studio` shown as `history_root`.
- C83 no telemetry consent state in the envelope.

Logging:

- C84 `crates/skill-studio-host/src/telemetry.rs:367-382` — Sentry tags only surface, outcome, error_code.
- C85 `telemetry.rs:360-366` — every Err becomes `InternalError` span status, caller errors too.
- C86 `telemetry.rs:1336-1349` — correlation_id not sent; a user report cannot be matched to a Sentry event.
- C87 0 breadcrumbs.
- C88 ✅ `SKILL_STUDIO_TELEMETRY=0|false` turns it off.
- C89 history has writes only; reads and pre-session refusals not recorded (except restore Failed row).
- C90 review writes not recorded (C60).
- C91 `main.rs:1765` — `resolve_target` lookup scan runs on its own ctx; one command shows as two operations.
- C92 `apps/cli/src/output.rs:38-46`, `main.rs:1870`, `:1884` — warnings to stderr; `ops.rs:6815` park git check swallows its own errors with `.ok()?`.
- C93 ✅ `crates/skill-studio-core/src/events.rs:443-447` — `CoreNotice::Recovered`.
- C94 `error.rs:124` — Io `source` "kept for logs only", but nothing logs it.

Primitives:

- C95 ✅ `exit_status` pure; core never reads HOME.
- C96 reads with side effects: usage writes cache, events/outdated take write runtime, park runs git.
- C97 missing JSON schemas: `ResultEnvelope`, `ErrorEntry`, `BatchParkRequest`, `BatchParkOutcome`, `BatchUnparkOutcome`, `UsageReport`, `HarnessesRequest`, `HarnessReport`, outdated request, `sweep_quarantine` outcome, health report, `WatchLine`, `ParkCheck`, `DiscardOutcome`.
- C98 bundled ops: `fix`, `turn_off_for_agent`, `update_all`, `doctor`, `invocation`, `park` (+git), `add` (+`save_as_preference`, `main.rs:1318`), `undo`, `resolve_target`, `MutationSession::begin` (recovery + reconcile + retention + scan, `ports.rs:1531-1576`).
- C99 `crates/skill-studio-core/src/fsops.rs:5` "Nothing in `crate::ops` calls these yet" — `ops.rs:4533-4539` calls them. Verified.

Fail modes and retries:

- C100 ✅ reads past budget return `Partial` (`ops.rs:441-443`, `:791-797`).
- C101 ✅ bad registry file reads as "nothing tracked" (`envelope.rs:534-535`, `ownership.rs:75-233`).
- C102 reads fail closed under a held write lease: `scope_busy` exit 3, not a partial result (`ports.rs:568-575`, `envelope.rs:301-303`).
- C103 ✅ writes fail closed (lease, recovery, confinement, missing history, `--confirm`, stale/drift).
- C104 `ops.rs:6815` — park git check fails open (warns only).
- C105 no in-process retry or backoff in core.
- C106 recovery retried on next write (`journal.rs:335-341`, `:652-653`, `ports.rs:1540-1546`).
- C107 **no retry cap and no way to drop a stuck plan from CLI or MCP**; a plan that keeps failing blocks every write with `incomplete` (`journal.rs:722-741`).
- C108–C110 ✅ recovery idempotent, newest first, continues past one failure, cleans temp links.
- C111 ✅ `crates/skill-studio-host/src/lease.rs:13`, `:20` — 20 ms poll, 500 ms stale takeover.
- C112 `events.rs:424-425` — reads see unrecovered state until the next write.

Components:

- C113 `ports.rs:88-115` — hand-written `confine`: lexical check, canonical parent, `scope.contains`; check-then-use race.
- C114 `fsops.rs:214` — `Root::confine` walks components by hand.
- C115 no cap-std; rustix `openat` NOFOLLOW only in `crates/skill-studio-host/src/probe_cache.rs:80-150`.
- C116 lease uses std `File::try_lock` + sysinfo; no fs2/fd-lock.
- C117 `journal.rs:672-678` — recovery ownership by path prefix.
- C119 same as D47 (npm README undo).
- C120 this file's earlier "A ✅" row was too strong (C4, C6, C25); corrected below.

## Desktop state ownership

Files: `D` = `apps/desktop/src-tauri/src`, `F` = `apps/desktop/src`,
`C` = `crates/skill-studio-core/src`, `H` = `crates/skill-studio-host/src`.

Main point: the desktop does not follow "one owner writes each state".
`events.sqlite3` has two schema owners and a third raw-SQL reader.
`~/.agents/skill-studio.json` has about 10 desktop writers and 3 core
writers. About 54 of 78 Tauri commands do not go through the core.

### Desktop writers (W)

| Id  | Where                                                        | Writes                                                                                                                                                                                                               | Core?                                    | Core twin                                                                                                              |
| --- | ------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| W1  | `D/skills/event_store.rs` (2075 lines)                       | `events.sqlite3` schema `:90-119`, migrations `:126`, `:146`, legacy import `:218-300`, `record` `:366`, `finish` `:401`, backups `:315-350`, restore `:610-1030`                                                    | partly (`FsJournal`, `backup_retention`) | `ops::list_events`, `ops::restore_event`, `H/history.rs:811`, `:851`                                                   |
| W2  | `H/history.rs:84-86`                                         | same file; "Kept in sync by hand"; has `backup_targets`, `expired_backups` (`:106-114`) the desktop lacks                                                                                                            | —                                        | —                                                                                                                      |
| W3  | `D/skills/skill_refresh.rs:1844-1860`                        | raw SQL read of `events` (third reader)                                                                                                                                                                              | no                                       | —                                                                                                                      |
| W4  | `D/skills/skill_materialize.rs` (1492)                       | links, removes `:48-1063`; events `:131`…`:990`. Lease taken by the command (`D/skills/event_commands.rs:511-512`)                                                                                                   | no                                       | none                                                                                                                   |
| W5  | `D/skills/skill_independent_copy.rs` (2397)                  | moves `:44-1111`; events `:147`, `:838`; registry                                                                                                                                                                    | no (takes lease)                         | only `EventKind::MakeIndependentCopy`                                                                                  |
| W6  | `D/skills/skill_fork.rs` (3909)                              | cache, extract, swaps `:1350-1440`, staging, registry; runs npx/gh; no events                                                                                                                                        | no (takes lease)                         | none; `:137` "`ops::install` doesn't cover"                                                                            |
| W7  | `D/skills/skill_frontmatter_repair.rs:505`, `:574-577`       | own lease, backups, event                                                                                                                                                                                            | propose only                             | `C/ops.rs:4061`, `:4159`                                                                                               |
| W8  | `D/skills/skill_md_write.rs` (177)                           | atomic SKILL.md write; process-local `SKILL_MD_WRITE_LOCK`; no event                                                                                                                                                 | no                                       | core `ops_invocation` also writes SKILL.md                                                                             |
| W9  | `D/skills/skill_harness_disable.rs:66-103`, `:175`           | legacy un-move; **no lease, no event**. Verified                                                                                                                                                                     | no                                       | none                                                                                                                   |
| W10 | `D/skills/skill_fork_registry.rs:169`, `:408`, `:485`        | `~/.agents/skill-studio.json`, own `ForkRegistry` shape                                                                                                                                                              | writer fn only                           | `C/ops_install.rs:183-199`, `C/ops_update.rs:597`, `:604`, `:1262`, `C/review.rs:90`                                   |
| W11 | registry callers                                             | skill_fork, skill_independent_copy, skill_frontmatter_repair, skill_trust_policy `:113`/`:133`/`:148`, skill_editor `:190`, telemetry_commands `:119`, harness_first_run, skill_refresh, skill_add_operation `:1342` | —                                        | —                                                                                                                      |
| W12 | `D/skills/skill_update_check.rs` (4175)                      | `update-check.json` `:217-231`; 6 h loop `:35`                                                                                                                                                                       | no                                       | `C/skill_update_check.rs:178` `outdated()` ("Ported from the desktop's … which this replaces"); desktop never calls it |
| W13 | `D/skills/skill_plugin_update.rs:312-319`                    | `plugin-versions.json`                                                                                                                                                                                               | no                                       | core plugin currency                                                                                                   |
| W14 | `D/skills/skill_install_counts.rs:193-197`                   | counts cache; errors ignored                                                                                                                                                                                         | no                                       | none                                                                                                                   |
| W15 | `D/skills/skill_editor.rs:408`, `:450`, `D/commands.rs:2141` | editor launch script                                                                                                                                                                                                 | no                                       | none                                                                                                                   |
| W16 | `D/skills/skill_process.rs:395`                              | process output file                                                                                                                                                                                                  | no                                       | none                                                                                                                   |
| W17 | `D/skills/skill_refresh.rs:249`                              | `let _ = remove_file(skill-invocations.json)`                                                                                                                                                                        | —                                        | —                                                                                                                      |
| W18 | `D/timing_log.rs:339`, `:354`, `:376`, `:479`                | `timing.jsonl` and rotation                                                                                                                                                                                          | no                                       | none                                                                                                                   |
| W19 | ✅                                                           | no desktop writes to `agents.toml`, `agents.lock`, `.skill-lock.json`, agent configs (`D/skills/skill_fork_registry.rs:3-5`)                                                                                         | —                                        | —                                                                                                                      |

Duplicated logic: update check (W12 vs core + `H/gh_currency.rs`); plugin
currency (W13); event store schema, list, restore (W1 vs W2); frontmatter
repair apply (W7); registry shape (W10 vs `RawRegistryDocument`); SKILL.md
writer (W8 vs `ops_invocation`); atomic temp-and-rename in 4+ places.

Tauri commands (`D/lib.rs:373-458`): 78. Through core about 24 (remove,
update, update_all, part of restore, detect_harnesses, fix, add, 7 park, 4
split/turn-off, invocation, snapshot/rescan). Not through core about 54
(list_skill_events; materialize, independent copy, repair link; fork, pull,
unfork; frontmatter preview/apply; restore_moved_deployment; SKILL.md
read/write; plugin enable/update/uninstall; check updates now; add-operation
start/cancel/get/confirm (not checked); skills.sh browse; editor; project
folders; registry settings; harness choice; telemetry; updater; status). Some
are reads or UI only and need no core op.

Restore split: `D/skills/event_commands.rs:242-254` `DESKTOP_OWNED_KINDS` go
to the desktop store; the rest to core `restore_event` (`:301`).
`list_skill_events` uses the desktop store (`:216`).

Migration plan: `docs/spec-headless-performance-observability.md:3` "Draft
… Not implemented"; `:45`, `:157` say the Tauri crate must not keep its own
copy of migrated ops; `:257` Phase 4 has no status column;
`D/skills/event_store.rs:163-168` "that adoption is a later slice".

### Desktop rules

| Rule                      | Verdict | Evidence                                                                                                                                                                                                                                                                                                                                                                                                                   |
| ------------------------- | ------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| S caching                 | 🟡      | Snapshot `RwLock` with watcher ✅; update check up to 6 h stale; counts write errors ignored; `LoginShellToolLookup` lives for the process (`D/skills/core_runtime.rs:90-95`); per-repo `OnceLock` tree cache (`D/skills/skill_update_check.rs:455-460`); revision guard in `F/hooks/useSkillSnapshot.ts` ✅                                                                                                               |
| S slow work off UI thread | ✅      | `time_command_async`/`_blocking`; refresh on its own thread; new-window routing on a worker                                                                                                                                                                                                                                                                                                                                |
| S push vs pull            | ✅      | watcher debounce 750 ms (`D/skills/skill_refresh.rs:46`), retry 5 s (`:59`); frontend "Never polls"; `Sidebar.tsx:282` 30 s tick only re-renders the age label; remote checks are timers (6 h, 4 h)                                                                                                                                                                                                                        |
| S events                  | ✅      | about 6 Tauri events, each one state stream                                                                                                                                                                                                                                                                                                                                                                                |
| S log unhappy paths       | 🟡      | `timing.jsonl` per command (first error line, 120 chars) ✅; refresh errors eprintln only (`skill_refresh.rs:1385`…`:1768`); updater failures eprintln only (`D/skills/skill_update.rs:231`, `:421`); silent: counts cache write, `skill_refresh.rs:249`, `useSkillInstalls.ts` `.catch(() => {})`, `useSkillSnapshot.ts` `.catch(() => undefined)`; breadcrumbs 0                                                         |
| S killswitches            | 🟡      | telemetry off by default, live toggle, env override ✅; **no user or env switch for auto-update** (auto-downloads, install needs a click); no feature flags                                                                                                                                                                                                                                                                |
| S retries, fail modes     | 🟡      | unread roots → `scan_partial` ✅; failed rebuild keeps old snapshot ✅; sticky "Try again" toast ✅; startup recovery failure blocks commands ✅; panic → Err ✅; registry/history writes `write_version` check + 5 s busy timeout, no retry; W9 no lease                                                                                                                                                                  |
| S boring components       | 🟡      | standard libs ✅; hand-written atomic writes ×4; second event store and journal; second update scheduler; process-local lock beside `FileLease`; hand-written textarea editor                                                                                                                                                                                                                                              |
| R11/R12 errors to user    | 🟡      | `callCommand` normalises (`F/lib/skill-api.ts:73-99`); about 98 toasts; `SkillCommandError.code` exists but **`D/skills/core_runtime.rs:185-193` `to_command_result` drops the core error code** (verified); only `event_commands.rs:310-313` sends a code; frontend string-matches "changed since"/"drifted"; React errors to Sentry ✅; backend command errors reach Sentry as status only; no `captureException` in `F` |
| R23 untrusted SKILL.md    | 🟡      | react-markdown + remark-gfm, no `rehype-raw`, no `dangerouslySetInnerHTML` ✅; only http/https links ✅; new windows denied ✅; **no `img` override or `urlTransform`, so `![](https://…)` in a SKILL.md loads a remote image** and **`tauri.conf.json:30` `"csp": null`** (both verified); store SKILL.md shown as `<pre>` ✅                                                                                             |

Desktop finding list (scan numbering): 1–7 event store (W1–W3, restore split,
list); 8–13 writers W4–W9; 14 registry W10–W11; 15–17 update and plugin
checks; 18–22 caches and files W14–W18; 23 no lock-file writes ✅; 24
command count; 25–26 migration plan status; 27–35 watcher, snapshot, status,
timers ✅; 36 updater eprintln; 37 no update switch; 38 telemetry switch ✅;
39 no feature flags; 40 Sentry breadcrumbs 0, status only; 41 error code
dropped; 42 one coded error; 43 string matching; 44–47 error UI ✅; 48 no
`captureException`; 49 swallowed `.catch`; 50–53 caches; 54 timing log ✅;
55–57 markdown and links ✅; 58 remote images; 59 `csp: null`; 60–61 ✅.

Scan claims corrected after checking: `skill_materialize.rs` has no lease
code, but its command takes a `WriteLease`, so W4 is not a lease gap.

## Fix backlog (all sources)

Order: first the items that can lose data or let an agent act on bad input,
then contract and error items, then docs. Each row names the finding ids.
Per AGENTS.md "Before you implement an issue": one need per issue; rare
cases become follow-ups.

### 1. Safety

| #   | Fix                                                                                                                                                                | Ids                     |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------- |
| B1  | Make the fixture scope block writes, or change the README claim. Prefer blocking: an agent sandbox must be safe.                                                   | C54, C55, D9, U sandbox |
| B2  | Give a way out of a stuck recovery plan: a retry count, then a `recovery` command that shows and drops the plan, with an event.                                    | C107, C112, C72         |
| B3  | Cap backup bytes (`MAX_BACKUP_BYTES`) and say what happens at the cap.                                                                                             | C38                     |
| B4  | Label third-party text for agents: mark SKILL.md body, description, frontmatter, paths and child stderr as untrusted in MCP output and tool descriptions.          | R23, U1–U36             |
| B5  | Desktop: block remote images in rendered SKILL.md (`img` override or `urlTransform`) and set a CSP.                                                                | R23, desktop R23 row    |
| B6  | Take a lease and write an event in `restore_moved_deployment`.                                                                                                     | W9                      |
| B7  | Use the core `FileLease` for SKILL.md writes in the desktop, not a process-local lock, so core `ops_invocation` and the desktop cannot write at the same time.     | W8                      |
| B8  | Replace hand-written confinement with `cap-std` (or openat NOFOLLOW) to close the check-then-use race.                                                             | C113–C116               |
| B9  | Bound bulk work: size limit for `update_all` and batch park; say what is left on a mid-batch failure; time budget for `scan_for_usage`; end condition for `watch`. | C34–C37, C39            |
| B10 | Make the park git check fail loud (warning in the envelope, not `.ok()?`).                                                                                         | C92, C104               |

### 2. One owner writes each state

| #   | Fix                                                                                                                                    | Ids                        |
| --- | -------------------------------------------------------------------------------------------------------------------------------------- | -------------------------- |
| B11 | Give the spec's Phase 4 a status column and a list of ops still in the desktop.                                                        | spec rows, W1–W18          |
| B12 | One events schema: the desktop uses `skill-studio-host` history; delete the desktop `CREATE TABLE`, migrations and the raw SQL reader. | W1–W3                      |
| B13 | Move restore and list fully to core; drop `DESKTOP_OWNED_KINDS`.                                                                       | W1, restore split          |
| B14 | Move materialize, independent copy, fork, frontmatter apply and the legacy un-move into core ops. One issue each.                      | W4–W7, W9                  |
| B15 | One registry writer: desktop calls core for `~/.agents/skill-studio.json`; one document type.                                          | W10, W11                   |
| B16 | Desktop uses core `outdated()` and core plugin currency; delete the desktop update check (4,175 lines).                                | W12, W13, Two writers      |
| B17 | One atomic-write helper for the remaining desktop files.                                                                               | W14–W18, Boring components |

### 3. Errors and exit codes

| #   | Fix                                                                                                                                          | Ids                               |
| --- | -------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------- |
| B18 | Keep the core error code in Tauri results (`to_command_result`) and switch the frontend from string matching to codes.                       | W rules R11/R12, `isDriftRefusal` |
| B19 | Add `not_found` and `internal` codes; give `io` and `execution_failed` their own exit status.                                                | C62–C64                           |
| B20 | Give `update_all` and batch errors codes and per-item entries; exit non-zero when every item fails.                                          | C65–C67, U28                      |
| B21 | One code for "already parked"; make repeat park, add and restore return ok with "already done".                                              | C14–C18, C71                      |
| B22 | Skip the backup and event when an invocation policy is already set.                                                                          | C19                               |
| B23 | Put every event id in the `fix` and invocation envelopes.                                                                                    | C22, C23                          |
| B24 | Name the item in vague messages (event id, skill name, path) and remove internal text.                                                       | C73                               |
| B25 | Pass `LeaseBusy` pid and age to the caller.                                                                                                  | C68                               |
| B26 | Run CLI error envelopes through `sanitized()`.                                                                                               | C70                               |
| B27 | Document exit 70 and give the npm shim's "platform package missing" its own exit status.                                                     | C75, C76                          |
| B28 | Proxy client: read the proxy's error body; drop "API key is invalid" in proxy mode; read `Retry-After` on every route; match the 60 s pause. | P18, P19, P52–P55                 |
| B29 | Fix the `domain.com/slug` detail-id mismatch between client and proxy.                                                                       | P5, P6, P61                       |

### 4. Logging and killswitches

| #   | Fix                                                                                                                   | Ids                            |
| --- | --------------------------------------------------------------------------------------------------------------------- | ------------------------------ |
| B30 | Add a `report_problem` MCP tool (and CLI command) that writes a local, scrubbed report with the correlation id.       | R25, D self-diagnostics        |
| B31 | Send the correlation id, binary version, and error code to Sentry; caller errors are not `InternalError`.             | C79, C84–C86                   |
| B32 | Record refused writes and pre-session errors in telemetry; record review writes in history.                           | C60, C77, C89–C91              |
| B33 | Desktop: replace `eprintln!` and swallowed `.catch` with logged, reported errors; send command error codes to Sentry. | Desktop log row, P51, P56, P57 |
| B34 | Proxy: report upstream 401 (bad server key) and 429; add a p95 or error-rate alert.                                   | P33, P35, P39                  |
| B35 | Killswitch for auto-update (setting plus env var).                                                                    | Desktop killswitch row         |
| B36 | Remote off switch or URL override for the proxy without a release.                                                    | P41, P42                       |
| B37 | Trace sample rate below 1, or say in the README that tracing is on.                                                   | P39, P72                       |

### 5. Contract and API shape

| #   | Fix                                                                                                                   | Ids                |
| --- | --------------------------------------------------------------------------------------------------------------------- | ------------------ |
| B38 | JSON schemas for the envelope, `ErrorEntry` and the 12 other missing types; snapshot test.                            | C6, C97            |
| B39 | Envelope goldens for each write.                                                                                      | C4                 |
| B40 | Add `next` cursor and `has_more` to `list_events`; limits for scan, diagnose, usage, outdated.                        | C24–C27            |
| B41 | Add a CHANGELOG and deprecation warnings for aliases.                                                                 | C12, C13, C7       |
| B42 | Envelope or schema_version for `health` and `watch`; remove or honour `add --global`.                                 | C9, C10            |
| B43 | Fill `timings` only on request; let `doctor` take a check list.                                                       | C29, C30, C33      |
| B44 | Reads under a held lease return a partial result, not `scope_busy`.                                                   | C102               |
| B45 | `events` and `outdated` use a read runtime; say in output that `usage` writes a cache.                                | C58, C59, C96      |
| B46 | Global `--dry-run` for writes; enforce MCP read-only hints in a test.                                                 | C51, C56           |
| B47 | Proxy: detail without file contents on request; range-check `limit`, `per_page`, `page`; size cap on upstream bodies. | P23, P24, P62, P64 |
| B48 | Proxy: rate limit after the cache; tighter limit on detail; `X-RateLimit-*` headers.                                  | P13, P14, P16      |
| B49 | Binary version, platform and telemetry state in the envelope.                                                         | C79, C83, R14      |

### 6. Tool surface (agents)

| #   | Fix                                                                                                                                                                                                 | Ids                                 |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------- |
| B50 | Remove overlaps: one repair tool (`fix` or preview/apply), one update tool, one turn-off tool with `confirmed`, one diagnose entry.                                                                 | R7, C42, T rows                     |
| B51 | One vocabulary: park/unpark only (drop enable/disable/turn-off names or make them aliases); `--agent` everywhere; one id flag; skill always `--skill` or always positional; `remove` not `discard`. | R9, C44–C50                         |
| B52 | Each tool description says read or write, what it returns, and has no Rust type jargon.                                                                                                             | T rows                              |
| B53 | Fix ignored params: `review_clear.date`, `review_due.skills`/`timings`; required `review_set.date` in the schema; `deny_unknown_fields`.                                                            | T review rows, `turn_off_for_agent` |
| B54 | `split` visible in the CLI or hidden in MCP, the same in both.                                                                                                                                      | C43                                 |
| B55 | `tools/list` snapshot test.                                                                                                                                                                         | V9, T cross-cutting                 |
| B56 | Ship a SKILL.md that teaches an agent to use Skill Studio (the tidy-up flow).                                                                                                                       | R17, A30, A31                       |

### 7. Tests

| #   | Fix                                                                                                 | Ids          |
| --- | --------------------------------------------------------------------------------------------------- | ------------ |
| B57 | Fix the dev-tool test list (`doctor` out, `install_preferences` in).                                | D50, V13     |
| B58 | Tests for `install_preferences`, `sweep_quarantine`, CLI `review_clear`, the instructions workflow. | V10–V12, V15 |
| B59 | `flow_parity` requires a flows.md row per tool and command; add the missing rows and tests.         | V2–V4, V6    |
| B60 | Run parity on the default surface too.                                                              | V7           |

### 8. Docs

| #   | Fix                                                                                                                                                                                                                                    | Ids          |
| --- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------ |
| B61 | MCP README: tool list, history, env table, lease root, binary path, sandbox claim, tests list, "no cache".                                                                                                                             | D1–D10       |
| B62 | CLI README: `--help` list, lease roots, discovery type name, exit lists.                                                                                                                                                               | D38, D41–D45 |
| B63 | Public docs: per-agent turn-off ships; undo refuses park; exit-1 lists; remove owners; dotagents installs; README command table.                                                                                                       | D19–D34, D47 |
| B64 | Specs: `mcp serve` → `mcp`; tool-name rule exceptions; command list.                                                                                                                                                                   | D12–D17      |
| B65 | Proxy README: query allowlist, cache key, tracing.                                                                                                                                                                                     | P71–P73      |
| B66 | AGENTS.md: one Rust command shape; versions; pnpm workspace; file paths; remove `_cachedSections`, `discover_all`; error rule matches spec D7; link the specs, verification docs and verify skill; shorter, with MCP and CLI pointers. | A5–A32, D18  |
| B67 | Fix the stale `fsops.rs` doc comment.                                                                                                                                                                                                  | C99          |
| B68 | Proxy: turn off `workers_dev` or say why it stays.                                                                                                                                                                                     | P75          |

### Process

- R20 (reuse subagents) conflicts with the T3 rule "every new review is a
  new `delegate_task`". No product change; note only.

### Kept as is (match)

Proxy auth and stateless design, edge cache, background counts, markdown
rendering without raw HTML, journal recovery, write refusals, `schema_version`,
error names fixed by test, file watcher, perf p95, and every row marked ✅
above. No action.
