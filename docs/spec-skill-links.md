# Skill links

Status: draft for review. Evidence:
[how skills in popular packs depend on each other](research/skill-links-evidence.md).

## 1. The need

**Before I remove, park, turn off or skip a skill, show me what stops
working.**

Skills name other skills. Ten of twelve popular packs checked on
2026-11-27 have links between their skills. Four shapes recur:

- A **wrapper**: `grill-me` (1.6M installs) is one line, `Call the Skill
  tool with "grilling".` Without `grilling` it does nothing.
- A **base skill**: 24 Lark skills open `../lark-shared/SKILL.md`; six Matt
  Pocock skills send the user to `/setup-matt-pocock-skills`. Parking the
  base breaks most of the pack.
- A **link across plugins**: pstack's `poteto-mode` says "Before commit →
  `/deslop` from the `cursor-team-kit` plugin". Installing pstack alone
  leaves that step with nothing to run.
- An **index**: `ask-matt` and `ask-emil` list their whole pack. Removing a
  listed skill leaves a dead menu entry; removing the index breaks nothing.

No agent and no installer shows this today. Skill Studio already knows every
copy of every skill, for every agent, in every scope, so it is the one place
that can answer the question for the user's own machine, whatever its layout.

## 2. What a link is

A link is a line in skill A's folder that names skill B. Each link keeps the
file, the line number and the line itself, so every surface can show why it
thinks A needs B.

### Evidence required for the first release

Use the instruction to establish **what A does**. Use the installation and
pack records to establish **what B is**. A literal Skill tool call or a path
to a sibling's skill file survives an uninstall because A still names the
target. These two forms can warn. A slash command or a name in bold can
appear on the map when an installed folder, local ledger, plugin listing or
checked pack identifies the skill. Show these as mentions in the first slice.
A command spelled out with its source plugin can also appear as a mention.
A menu entry or a formatting choice alone never establishes a broken workflow.

For example, original pstack directs the reader to `**no-comments**` before
review. Keep that reference when a remaining pack record identifies the removed
skill. A hand-copied folder has no permanent membership record. When neither
the source nor the machine supplies that evidence, the map cannot identify
every target. Preserve a quoted slash command or a bold hyphenated name
in an exact lookup for that name or under **Show unmatched text**, with
the line where it occurs. Omit these text matches from the default report and exclude them
from graph edges and broken counts. Thus removing the last record for
`no-comments` still leaves its source line available, while the built-in
`/loop` cannot create a missing skill. A remove preview uses the same known names before and after the
proposed action. A scan never contacts the network to fill a gap.

The stronger prose examples and preview copy below describe later slices.
Promote a command to a warning only once its target and invocation can be
verified for that agent. A fork with a similar name supplies no rename
evidence. Report a rename only when the caller's pack and a visible install
in the relevant scope have a record connecting the upstream skill to its
installed name.

### 2.1 Strength

Strength decides where a link is shown. It is decided per line.

| Strength  | Evidence in the first release                                                                     | Presentation                        |
| --------- | ------------------------------------------------------------------------------------------------- | ----------------------------------- |
| Needs     | A sibling path names the other skill's file. Optional reads are mentions.                         | Skill page, map and removal preview |
| Hands off | The instruction passes a literal target to the Skill tool.                                        | Skill page, map and removal preview |
| Mentions  | A recorded target appears in prose, a slash command, a qualified name or an optional instruction. | Related skills                      |
| Lists     | An index includes a mention on a list or table row.                                               | Menu entry                          |

Words such as _load_, _first_, _run_ and _invoke_ are candidates for future
detection work. They do not promote a mention to a warning in this release.

An **index** names at least 5 skills and at least half of its pack, and at
least half of those lines are list items or table rows. On the checked
packs this finds `ask-matt`, `ask-emil` and pstack's `poteto-help`, and
rejects `executing-plans` (a workflow that hands off from prose). Each name on a line is classified on its own, so one line
can run one skill and mention another.

### 2.2 What is not a link

Each rule below comes from a wrong match in the evidence:

- A bare word. `teach`, `research` and `pdf` are ordinary words.
- A path that resolves inside A's own folder. `remotion-best-practices`
  carries its own copy of each sibling's guide, so its
  `../remotion-studio/REFERENCE.md` is internal.
- Text inside a bundled copy (a subfolder of A named after B).
- A line that tells the agent not to use B (`不要调用 lark-approval`, "do not
  call").
- A name used as a command: `npx hyperframes`, `caveman skills install`.
- A back reference ("called by `azure-app-onboard`"). It is recorded on B's
  side as a Mentions, never as A needs B.

Measured on 40 random links before the negation and command rules existed:
37 pointed at the right skill in the right direction, and those two rules
were written for the 3 that did not. The detection is a heuristic, which is why the line
always travels with the link.

### 2.3 Targets

| Target           | Example                                                     | What the user sees                                   |
| ---------------- | ----------------------------------------------------------- | ---------------------------------------------------- |
| Installed skill  | `grilling` in the same scope                                | Its name; the link state from section 3              |
| Missing, in pack | `ask-matt` names `pr`, which the pack ships but is left out | "Not installed" and Add, through `pack_add`          |
| Missing, unknown | A name no installed skill or known pack has                 | "Not found". Mentions of unknown names are not shown |
| Another pack     | `poteto-mode` names `/deslop` from `cursor-team-kit`        | The skill and its pack; "Not installed" if absent    |
| Not a skill      | A subagent (`poteto-agent`), a CLI (`npx hyperframes`)      | The kind and the name, never a state                 |

The pack a missing name belongs to comes from the pack's own listing
(`pack show --remote`), never a guess from the name.

## 3. Per agent, per machine

The same pack gives different answers on different machines. A link is
resolved for each agent that has a copy of A, using that agent's folders, so
the answer fits the machine it runs on.

- **Needs (path)**: resolve the path from the folder the agent reads A from.
  On a machine where `~/.claude/skills` is a link to `~/.agents/skills`,
  every `../` path resolves for Claude Code. On a machine with per-skill
  copies in `~/.claude/skills`, `../lark-shared/SKILL.md` resolves only if
  `lark-shared` is copied there too. Claude Code does not read
  `~/.agents/skills`; Codex, OpenCode, pi, Cursor and Grok Build do, so the
  same copy can resolve for one agent and not another. Project skills
  resolve inside the project's folder.
- **Hands off (by name)**: B must be in the agent's own list: a copy it
  reads, not parked, not turned off for it, and not hidden by the agent's
  own settings (a Codex `skills.config` entry, an OpenCode permission),
  which the listing module already reads. A file that opens is not proof
  the agent can invoke the skill by name. When the settings cannot be read,
  the state is Not known. A plugin install answers to its
  plugin's prefix too: Claude Code lists pstack's skills as
  `pstack:poteto-mode`.
- **Per scope**: an agent runs a skill either globally or inside one
  project. Each link is resolved per agent and per scope, for every project
  the scan covered, using the copy that wins in that scope (the pi resolver
  in `crates/skill-studio-core/src/listing_visibility/pi_listing.rs`
  already picks it). A global wrapper whose target one project overrides
  gets one state for that project and another for the rest. The report
  names the scopes it covered. A global result applies across those projects
  unless a project row gives a different result. Count a broken link once
  per affected agent.
- **Renamed installs**: resolve the literal name first. If it is absent,
  show Renamed when the installation record maps this pack's upstream
  skill to a visible copy under a different name. Otherwise show what the
  literal lookup found. Shared prefixes and suffixes carry no identity.

Link states, worst first. A skill's card and row show its worst state:

1. **Missing**: no copy for this agent.
2. **Renamed**: a copy exists under another name, and A asks by name.
3. **Off**: parked, or turned off for this agent.
4. **Elsewhere**: a copy exists in another agent's or another scope's
   folder only.
5. **Not known**: the agent's settings or the target's folder could not be
   read.
6. **OK**.

**Not known is not none.** A plugin pack whose files Skill Studio cannot
read, a folder the scan truncated, or a pack not yet checked upstream gives
"links not known", never "no links". Names built by a rule (pstack's
"principle-<title> in kebab case") are not found; a skill that uses one is
marked "may name skills by rule" when its text has `<` placeholders next to
the word skill.

## 4. Core contract

One core module, `crates/skill-studio-core/src/skill_links.rs`, with no
filesystem access of its own (like `skill_uses.rs`): a host adapter hands it
the text of each skill folder through the scope port.

- **Operation** `skill_links(scope, filter)`. `filter` takes a skill name,
  a pack source, an agent, or `missing_only`. The result lists links
  (`from`, `to`, `strength`, `target_kind`, `evidence` with file, line and
  text, and `states`: the global state and project-specific differences
  for each agent, so one report can say OK globally and broken in one project) and a summary (`skills`, `links`, `missing`,
  `not_known`).
- **A preview before every change, on every surface.** Today remove and park
  have no CLI dry run, and the MCP remove and park tools change files
  directly; only turn-off has a check (`turn_off_check`). A read-only
  preview is added for the exact action and targets: remove, park
  (including batch park), turn-off, pack remove, and pack add with a
  subset. The CLI takes `--check`, which prints it and changes nothing; the
  MCP tools take `check: true` and return it. An interactive CLI run prints
  it before asking to confirm. A `--json` or MCP caller gets it in the
  response and calls again without `check` to apply. The desktop dialogs
  show the same preview, and their destructive button stays off until the
  preview has loaded or shows an explicit "could not tell". Every surface
  reads one response shape, with these lists per agent and scope:
  - `breaks`: skills that stay installed and have a Needs or Hands off link
    the action turns from OK to Missing, Renamed, Off or Elsewhere.
  - `through`: skills that stay installed whose own links still resolve but
    reach a skill in `breaks` through a chain of Needs and Hands off links,
    so their workflow loses a step. A removed copy that another copy still
    covers breaks nothing and adds nothing here.
  - `unmet`: skills the action installs whose Needs or Hands off targets
    will not be reachable.
  - `unknown`: links the action turns from OK to Not known. Never a warning.
  - `fixes`: links the action turns from anything else to OK.

  The preview also has a `status`: `complete`, or `not_known` with a reason
  (offline, a folder that could not be read). A `not_known` preview is not
  a clean result: its lists hold only what could be checked, and the
  desktop shows "could not tell" instead of "nothing breaks".

- **Doctor**. One new issue kind, `MissingLink`, for a Needs or Hands off
  link in state Missing or Renamed. It shows in `doctor`, on Home, and in
  `skill-studio doctor --json`.
- **Cost**. The checked packs hold 15 MB of Markdown across 316 skills, 5.2
  MB for the largest pack. Extraction records every candidate name form in
  a folder (backticked, bold, slash, quoted, path folder) without looking
  at the inventory, and is kept per folder by its content hash, so a scan
  rereads only changed folders. Matching candidates to installed names and
  resolving them runs on every scan, because installing a target changes
  the answer for an unchanged wrapper.
  Names are matched in one pass per file over all names in scope
  (`memchr`, already a core dependency; `aho-corasick` is in `Cargo.lock`
  through `regex` but would be a new direct dependency). The work rides the
  existing `ScanMemo` (`crates/skill-studio-core/src/scan_memo.rs`), which
  already cut a no-change desktop rescan of 529 skills from 1,350 to 350 ms
  by not rereading unchanged folders (`docs/perf/README.md`). Proposed
  budgets, to be measured with `pnpm run perf:baseline` on the real-shaped
  profile plus the real-pack snapshots before slice 1 merges:
  - No-change rescan: zero extra file reads for links.
  - First scan: links add at most 10% to scan time.
  - Dialogs: to be measured. `ScanMemo` keeps a folder's result for at
    most 10 minutes, and `park_check` scans when called, so a preview can
    cost a scan. The target is the 100 ms p95 input budget in
    `docs/spec-headless-performance-observability.md`; if a scan does not
    fit, the dialog shows its plan first and keeps its destructive button
    off until the preview arrives.
  - Store: no extra network. The SKILL.md the detail panel already fetches
    is the only text read before install.
  - Map: layout runs only when the map opens, and again only when its shape
    key changes. The largest checked pack has 59 skills.
- **Errors**. Follow D7 in `docs/spec-core-primitives.md`. An unreadable
  file is a `not_known` reason on that skill, not an error for the call.

## 5. Surfaces

Every surface reads the same core result. A link never shows without its
line one click away.

### 5.1 Before a change (preview slice)

The remove, park, turn-off-for-agent and pack remove dialogs gain a
"What stops working" section from the plan's `breaks`:

> **Remove `grilling`?**
> 5 skills call it: `grill-me` (does nothing without it), `grill-with-docs`,
> `improve-codebase-architecture`, `triage` and `wayfinder`.
> [Keep] [Remove anyway]

The Add Skill sheet and pack add, when the user picks a subset, show what
the picked skills need and offer to add it:

> `grill-with-docs` runs 2 skills you did not pick: `grilling`,
> `domain-modeling`. [Add only mine] [Add all 4]

### 5.2 On the skill page

A **Links** card with two lists, **Uses** and **Used by**, grouped by
strength, each row with the target's state for the selected agent and its
line on hover or focus. Mentions sit under a closed "Related" row.

### 5.3 Home

- A **Missing links** group under Warnings: one row per skill with a Needs or
  Hands off link that is Missing or Renamed, with Add when the target's
  pack is known.
- **Not used in 30 days** never offers to park a skill that a used skill
  needs. The row says why, with the count of the user's runs of the skills
  that need it: "Called by `grill-me`, which ran 14 times."

### 5.4 Pack map

Section 6.

### 5.5 CLI

```
skill-studio links [<skill>] [--pack <source> | --path <dir>] [--agent <id>]
                   [--missing] [--project-path <path>] [--json]
```

```
$ skill-studio links grilling
grilling  (mattpocock/skills)
Called by 5 skills:
  grill-me                        hands off  SKILL.md:7   Call the Skill tool with "grilling".
  grill-with-docs                 hands off  SKILL.md:7   Call the Skill tool twice, for "grilling" and "domain-modeling".
  ...
Listed by: ask-matt
Removing it breaks: grill-me (directly), 4 others (a step each)
```

`remove`, `park`, `turn-off` and `pack remove` take `--check`, which prints
the preview of section 4 and changes nothing.

### 5.6 MCP

One tool, `skill_links`, with the same input and output as the CLI's
`--json`. Its description tells the agent to call it before it removes,
parks or turns off a skill, and to show the user what would break. The
remove, park, turn-off and pack tools take `check: true` and return the
preview; their descriptions say to call with it first.

### 5.7 Every starting point, every action

**The model.** A link's state (OK, Missing, Renamed, Off, Elsewhere, Not
known) is derived per link and per agent from the files on disk. It is never
stored (rule 4 of `docs/state-machines.md`). Every action, whoever takes it
(Skill Studio, the skills CLI, dotagents, an agent, the user in Finder),
changes files, and the next scan derives the new states. So every flow comes
down to three questions: what the user sees before the action, after it,
and after undo.

**Five rules hold for every combination:**

1. **Before.** Every action Skill Studio runs shows its preview (section
   4): `breaks`, `through`, `unmet`, `unknown` and `fixes`, per agent and
   scope. Nothing else.
2. **After.** The next scan shows exactly the links that changed state. A
   change made outside Skill Studio shows the same way, marked "changed
   outside Skill Studio".
3. **Predicted equals observed.** Each preview list equals what the scans
   before and after the action derive: links that left OK for a confirmed
   loss, chains that reach those, unmet targets of installed skills, links
   that became Not known, and links that became OK.
4. **Undo is exact for journaled actions.** Undoing a Skill Studio action
   that recorded an event returns the links that action changed to their
   earlier state; links changed by later, unrelated actions stay as they
   are. A change
   made outside Skill Studio cannot be undone; the next scan reports it.
5. **Only real breaks warn.** Needs and Hands off links warn. Mentions,
   index entries and Not known never do.

**Starting points:**

| Start                         | Real example                                                    |
| ----------------------------- | --------------------------------------------------------------- |
| Nothing installed             | A first install from the Store                                  |
| The whole pack                | `npx skills add mattpocock/skills`                              |
| One skill                     | `grill-me` alone                                                |
| A subset                      | `grill-with-docs` and `to-spec`                                 |
| The pack minus some           | A dotagents wildcard with `exclude = ["pr", "teach", …]`        |
| Two packs that link           | pstack and `cursor-team-kit`, both from `cursor/plugins`        |
| One skill from two installers | A skills.sh copy and a Claude Code plugin copy of the same name |
| A renamed port                | `sergical/pstack-skills`, or a `matt-` prefix                   |
| Global and project            | `grilling` global, `grill-me` in a project                      |
| Edited copies                 | A pack skill with local edits                                   |

**Actions, and what the user sees.** The columns are the role of the skill
the action touches: one other skills need (`grilling`), one that needs
others (`grill-me`), or an index or standalone skill (`ask-matt`).

| Action                                                                                                    | On a skill others need                                                                                                                                                                                                                                            | On a skill that needs others                                                                                                                                                                                            | Index or standalone                               |
| --------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------- |
| Install the whole pack                                                                                    | Clears any Missing rows it fixes                                                                                                                                                                                                                                  | Offers only needs outside the pack (pstack's `/deslop`)                                                                                                                                                                 | Silent                                            |
| Install one skill or a subset                                                                             | "Fixes N links" when installed skills were missing it                                                                                                                                                                                                             | Offers everything it needs, through other skills too, ticked for Needs and Hands off                                                                                                                                    | Silent                                            |
| Remove                                                                                                    | Lists every skill that stops working, per agent, direct and indirect; Keep or Remove anyway                                                                                                                                                                       | Nothing breaks; silent                                                                                                                                                                                                  | Silent                                            |
| Park, unpark                                                                                              | As Remove, worded "until you unpark"; unpark lists what it fixes                                                                                                                                                                                                  | Silent                                                                                                                                                                                                                  | Silent                                            |
| Turn off or on for one agent                                                                              | Lists what stops working in that agent only                                                                                                                                                                                                                       | Silent                                                                                                                                                                                                                  | Silent                                            |
| Edit SKILL.md in the editor                                                                               | Changing its own `name:` makes every skill that asks for it by name unreachable if their agent uses that name; show Renamed only with a recorded mapping                                                                                                          | A new line naming a missing skill is marked in the editor and goes Missing after save; a typo is "Not found" with the nearest installed name offered; deleting the only line that names a skill drops the link silently | Silent                                            |
| Edit outside Skill Studio                                                                                 | The same outcomes after the next scan, marked "changed outside"                                                                                                                                                                                                   | Same                                                                                                                                                                                                                    | Silent                                            |
| Make an independent copy or fork                                                                          | Same name: nothing breaks. New name: resolve the old name again; show Renamed only when its record maps to the new copy                                                                                                                                           | Keeps its links                                                                                                                                                                                                         | Silent                                            |
| Split                                                                                                     | Split copies the whole skill to the chosen agents and removes the shared copy, so an agent not chosen loses its name-based hand-offs to it unless it reads a chosen agent's folder (Cursor, OpenCode and Grok Build read Claude Code's); listed in the split plan | Each chosen agent's copy keeps all its links                                                                                                                                                                            | Silent                                            |
| Move between project and global                                                                           | Skills that read it by path go Elsewhere; listed                                                                                                                                                                                                                  | Its path reads into the old scope's skills go Elsewhere; listed                                                                                                                                                         | Silent                                            |
| Update one skill                                                                                          | If a record maps the old name to the new install, show Renamed; listed in the update plan                                                                                                                                                                         | "Now needs X" with Add, if upstream added a link; silent if it dropped one. Edited copies are skipped and keep their old links (existing rule)                                                                          | Silent                                            |
| Update the pack                                                                                           | As above for each skill                                                                                                                                                                                                                                           | The skills CLI does not add new upstream skills on update, so a skill that now hands off to a new one goes Missing; the plan says so and offers Add                                                                     | Index lists new names as dead entries until added |
| Upstream deletes a skill                                                                                  | The local copy stays and nothing breaks. Check upstream says "Removed upstream; N of your skills still need it" so a cleanup does not break them                                                                                                                  | Silent                                                                                                                                                                                                                  | Silent                                            |
| Switch the pack to a fork                                                                                 | If the fork lacks a skill, the skills that need it go Missing; in the plan                                                                                                                                                                                        | Same                                                                                                                                                                                                                    | Silent                                            |
| Remove the whole pack                                                                                     | Lists skills in other packs that need it. A plugin pack is removed by its agent, so its row shows the agent's command and the same list                                                                                                                           | Silent                                                                                                                                                                                                                  | Silent                                            |
| Remove outside Skill Studio (Finder, `npx skills remove`, a dotagents `exclude` then `dotagents install`) | No warning is possible before. After the scan: new Missing rows, marked "changed outside", with Add                                                                                                                                                               | Silent                                                                                                                                                                                                                  | Silent                                            |
| A new agent appears (the user installs Codex)                                                             | Links are resolved for it; only its new breaks show                                                                                                                                                                                                               | Same                                                                                                                                                                                                                    | Silent                                            |
| Undo a journaled Skill Studio action                                                                      | Only the links the undone action changed return to their earlier state; their rows clear                                                                                                                                                                          | Same                                                                                                                                                                                                                    | Same                                              |

Each cell is the same five rules applied to that action. The first delivery
covers remove, park, turn-off and subset install on all three surfaces
(section 9); the editor, fork, split, move, update and outside-change rows
are the target for later slices. The worked
examples below are single rows of this table.

**A. Install `grill-me` from the Skill Store.**

1. The user opens `grill-me` in the Store. The detail panel already fetches
   its SKILL.md through `get_skill_details`.
2. Skill Studio reads that one file and finds `Call the Skill tool with
   "grilling"`. It checks the chosen scope and agents: `grilling` is not
   there. Under Install: "Runs `grilling`, which you don't have." The line
   shows on hover. Because only SKILL.md is read before install, a skill
   whose other files name skills says "Checked SKILL.md; the rest after
   install".
3. The install footer has "Also add `grilling` (mattpocock/skills)", ticked,
   because `grill-me` does nothing without it. A Mentions link is offered
   unticked; a Lists link is not offered.
4. Install runs one `npx skills add mattpocock/skills --skill grill-me
   --skill grilling`, the existing one-call-per-source batching. Each skill
   gets its own undo event.
5. The watcher rescans. Only the two new folders are read.
6. The `grill-me` page's Links card: Uses `grilling`, OK in Claude Code and
   Codex.

Outcome: a working `grill-me`, one extra tick box, no extra screen. If the
user unticks, `grill-me` installs alone and Home shows one Missing links row
with Add.

**A2. Same install, Claude Code only, `grilling` already in
`~/.agents/skills`.** Codex reads that folder, Claude Code does not. The
footer says "`grilling` is installed, but Claude Code can't see it" and
shows no button. No action today adds Claude Code to an existing shared
copy: pack add skips a skill already installed in the scope, and the skill
page shows no Claude Code switch for that copy. Slice 3 adds one, through
the per-agent link the install code already makes.

**B. Install from the CLI or an agent.** Today a named-subset
`pack add --dry-run` does not read upstream text, and `pack show --remote`
keeps only metadata. Slice 3 makes the plan fetch the source once (the same
shallow clone `pack add` without `--skill` already makes), keep the
checkout for the plan, and read the picked skills' Markdown. Offline,
the preview's `status` is `not_known` with the reason "offline". Then
`skill-studio pack add mattpocock/skills --skill grill-me --dry-run` lists
the plan with an `unmet` entry: "`grill-me` runs `grilling`, which is not
installed. Add it with `--skill grilling`." A real run without it installs
`grill-me`, prints the same line, and exits 0: a missing link is a warning,
not a failure. The MCP `pack_add` result carries the same `unmet` list, and
the tool description tells the agent to ask the user before it adds more.

**C. A month later, tidying up.** Home's Not used in 30 days would list
`grilling` if nothing ran it directly. Because `grill-me` ran and needs it,
the row says "Called by `grill-me`, which ran 14 times" and offers no park.

**D. Remove `grilling` anyway.** The skill page's Remove runs the plan. The
dialog lists the five skills with literal calls to it. `loop-me` also
mentions it. Keep changes nothing. Remove anyway removes it. Undo restores
the target, and the same five calls resolve again.

**E. Turn off `setup-matt-pocock-skills` for Codex.** Six skills direct
users to the slash command. Their source lines stay available under Related.
The preview counts confirmed calls and file reads separately.

**F. Update a pack whose links changed.** Check upstream finds an update.
The update plan lists link changes next to file changes: "`to-spec` now
hands off to `codebase-design`, which you don't have," with Add in the same
dialog.

**G. Install pstack as a Claude Code plugin.** `poteto-mode` says to run
`/deslop` from `cursor-team-kit`. That plugin is not installed, so the Links
card shows Missing, in `cursor/plugins`. Plugins are installed by the agent,
so the row shows the agent's command to copy, as plugin packs already do.

**H (later). A pack author checks a pack before publishing.**
`skill-studio links --path ./skills` reads a local folder as a pack and
lists every name that points at nothing in it, for example a skill renamed
in one place and not another. Exit status 1 when any Needs or Hands off link
points at nothing, so it can run in the author's CI.

## 6. The pack map

### 6.1 What we take from T3 Code's Environments map

T3 Code's map (Settings → Environments,
`apps/web/src/components/settings/environmentsMap.logic.ts` and
`EnvironmentsMap.tsx` in `pingdotgg/t3code`, read on 2026-11-27) shows
machines, the routes that reach them and the links between them. What makes
it good is how it thinks, not how it looks. Each row below is one decision,
why it works there, and what it becomes here.

| T3 Code decision                                                                                                                                        | Why it works                                                                | Skill Studio                                                                                                                                                                                          |
| ------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| The graph is built from what the client knows. It never probes, and a list it could not read is "not known", never "no links".                          | The map never lies about a machine it cannot see.                           | Links come from local text. An unreadable plugin or an unchecked upstream says "links not known".                                                                                                     |
| One node per machine, however often it is named.                                                                                                        | A machine is one thing to the user.                                         | One node per skill per scope. Its copies across agents are one node with a state per agent.                                                                                                           |
| Edge states are ranked worst first, and a card wears the worst.                                                                                         | The eye goes to what needs attention.                                       | Link states from section 3, worst first.                                                                                                                                                              |
| Two links between the same machines at the same access are drawn as one line with two arrowheads.                                                       | Half the lines, same meaning.                                               | Two skills that name each other at the same strength (siblings) are one line.                                                                                                                         |
| Many machines with the exact same incoming links become one stack: "a thousand VMs under two hosts cost one node and two lines".                        | The map stays readable at any size.                                         | Skills with the exact same set of skills linking to them become one stack (up to 5 skills share one set in `mattpocock/skills` and `azure-skills`). Unlinked skills fold into one chip past 12 nodes. |
| Focus lights one hop, because a link does not chain: A linked to B linked to C gives A nothing on C.                                                    | It draws only what is true.                                                 | The same honesty gives the opposite rule: a Needs or Hands off link does chain. If C goes, B fails, so A's step fails. Focus lights direct links strong and indirect ones faint.                      |
| Actions on the map (drag a plus onto a machine to link) use the same rule as the dialogs.                                                               | One rule, so the map and the dialog never disagree.                         | Remove, park and Add from the map open the same plan dialogs with the same `breaks`.                                                                                                                  |
| Columns that mean something: this client, its environments, machines known only through links.                                                          | Position carries meaning before the user reads a label.                     | Columns by role: index badge on top, then **Start here** (skills nothing in the pack names), **Steps**, **Base** (skills named by several and naming few).                                            |
| A shape key: the layout changes only when membership, names or wiring change, never on a status change.                                                 | The map does not jump while the user reads it.                              | The same: a state change recolors, a link change relays out.                                                                                                                                          |
| A status line with two numbers, a legend ("How to read the lines"), and a help popover of four rules.                                                   | The user learns the map in one look.                                        | "39 skills · 43 links" for `mattpocock/skills` (index entries are not links), plus "· N missing" when any are. Legend: solid = needs, arrow = hands off, dotted = mentions.                           |
| Search with a match count, arrow keys between columns, Esc clears the selection before it closes Settings, zoom and fit.                                | Keyboard users get the whole map; Esc never loses work.                     | The same, with `/` to search.                                                                                                                                                                         |
| Every node is a real button over an `aria-hidden` SVG of lines, and the details panel lists every member of a stack.                                    | The drawing is decoration; the buttons and the list are the accessible map. | The same, and the Links card (5.2) is the list view of the selected skill.                                                                                                                            |
| Layout is pure and tested: nothing overlaps, the same map whatever order data arrives in, 1000 machines as a few stacks, legible from 560 to 1400 wide. | Those are the rules a user notices when broken.                             | The same four tests, on the fixture packs of section 7.                                                                                                                                               |

What we do not take: T3's map is a fleet manager with routes, access levels
and drag-to-link. Skill Studio has no routes, and drawing links between
skills is not a user action. Users do not create links; authors write them.

### 6.2 What is ours

- **Show for: one agent.** A switch above the map: All agents, or one
  agent. The same pack can be fine for Claude Code and broken for Codex.
  This is the view no other tool can give.
- **Inside a pack, and across packs.** The map is per pack. Links to other
  packs end at a small card on the right edge naming the other pack; a
  missing one is drawn hollow.
- **The line is the proof.** Selecting a link shows its line, file and
  number in the details panel, with Open in the editor.

### 6.3 Where it lives

- Packs page: each pack row gets **Map**, which opens the map in the page.
- Skill page: the Links card has **Show on map**, which opens the pack's
  map focused on that skill.
- The layout code lives in a shared package, not in the desktop app, so the
  marketing site draws the same map from a fixture (section 8).

## 7. Real packs, built setups, no one's machine

The product, the harness, the demo and the video use real open-source
packs, the ones people already install and recognize. Installs and session
history around them are built for the purpose. Nothing reads a real home, a
real project, or a real person's session history.

**Real packs.** Pinned snapshots of four open-source packs, each kept with
its own `LICENSE` file, in `crates/skill-studio-core/tests/fixtures/real-packs/`,
copied by `tools/refresh-real-packs.sh`, which the core tests, the desktop
harness, the CLI checks and the marketing site all read:

| Pack                             | Commit    | License, holder    | Markdown | Why it is in                                  |
| -------------------------------- | --------- | ------------------ | -------: | --------------------------------------------- |
| `mattpocock/skills`              | `bbadaa6` | MIT, Matt Pocock   |  0.29 MB | Wrapper (`grill-me`), base skills, an index   |
| `cursor/plugins`, `pstack/` only | `b7b93c3` | MIT, Lauren Tan    |  0.28 MB | An index, links across plugins (`/deslop`)    |
| `obra/superpowers`               | `22f0b46` | MIT, Jesse Vincent |  0.35 MB | `pack:name` hand-offs                         |
| `emilkowalski/skills`            | `64eb3de` | MIT, Emil Kowalski |  0.36 MB | Path reads, siblings, an index that lists all |

- Only the skill folders' Markdown is kept, plus, for one real plugin
  install of pstack, `cursor/plugins`' `.claude-plugin` marketplace and
  plugin manifests and `cursor-team-kit`'s `deslop` skill, so the
  cross-plugin link is checked once with its target present and once
  absent. A README in the folder names
  each pack's source, commit and license, and one script refreshes a pack to
  a new commit. A refresh reruns the corpus test below, so a pack that
  changes its links changes the test on purpose, not by surprise.
- Packs without a license file (`remotion-dev/skills`, `anthropics/skills`,
  `vercel-labs/agent-skills` on 2026-11-27) are never copied in. They can
  appear by name and by the numbers in the evidence file.
- Every surface that shows a pack's text names its pack and license (the
  map's details panel: "MIT, Matt Pocock"), and the marketing site has a
  credits line linking each pack.

**Core fixtures for what real packs do not show**
(`crates/skill-studio-core/src/testing_shapes.rs`, generic names as that
file requires). These cover machine layouts and install states, which come
from the user's setup, not from a pack:

| Shape          | Built from                                                                | The test checks                                                                 |
| -------------- | ------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| Wrapper        | `skill-a` whose body is one Skill tool call to `skill-b`                  | Removing `skill-b` breaks `skill-a`                                             |
| Base skill     | `skill-base` read by `../skill-base/SKILL.md` from 6 skills               | Parking it lists all 6                                                          |
| Index          | `skill-index` with a table naming 8 skills                                | Lists, no breaks, no alerts                                                     |
| Bundled copies | `skill-router` holding its own `skill-x/REFERENCE.md` copies              | No links to the siblings                                                        |
| Not links      | "do not call `skill-b`", `npx skill-b`, "called by `skill-c`"             | No links                                                                        |
| Left out       | A dotagents wildcard pack with one `exclude`, named by an installed skill | Missing, with Add from the pack                                                 |
| Renamed        | A recorded alias asked for by its upstream name                           | Renamed                                                                         |
| Across packs   | A link from one pack's skill to another pack's skill                      | Target pack named                                                               |
| Agent layouts  | The same pack as a whole-folder link, per-skill links, per-agent copies   | Path links resolve per layout; turning off for one agent breaks only that agent |
| Project scope  | The base skill in the project, its users global                           | Elsewhere                                                                       |

**Desktop harness** (`apps/desktop/src/dev/harness`): the fixture estate
installs the real packs, set up the way real people install them:

- `mattpocock/skills` installed through skills.sh as a subset
  (`npx skills add mattpocock/skills --skill …`) that leaves out `pr`,
  `teach` and `improve-codebase-architecture`, with per-skill links for
  Claude Code. `ask-matt` and `setup-matt-pocock-skills` still name them,
  so the map shows real dead entries and one real Missing hand-off.
- `setup-matt-pocock-skills` turned off for Codex, so Show for Codex lights
  the skills that send the user to it. This works on a skills.sh install in
  the per-skill-link layout. The core refuses it for a dotagents-managed
  skill and splits nothing through a whole-folder link, so the demo does
  not use either for this beat.
- A project with a dotagents wildcard and `exclude`, for the left-out-skill
  case, without turning anything off.
- pstack installed from `cursor/plugins` without the `cursor-team-kit`
  plugin, so `poteto-mode`'s `/deslop` step shows as Missing, with Add from
  `cursor/plugins`.
- Simulated session history in `mock-sessions.ts`: `grill-me` run 14 times
  in 30 days, `grilling` never started directly. This is what makes Home's
  "not used" row and its guard real on screen.

**Real CLI runs**: the `verify-skill-studio` skill's local git pack server
serves the `mattpocock/skills` snapshot. The real `npx skills add` installs
`grill-me` alone into a disposable home, and `skill-studio links grill-me`
must report `grilling` as Missing, with Add from the pack.

## 8. Launch and homepage

The case study shows one moment with skills the viewer already has: they
tidy up, and Skill Studio stops them from breaking a pack they did not know
was connected.

**Video, three beats, on the harness with the real packs:**

1. Home lists `grilling` as not started in 30 days. The row says "Called by
   `grill-me`, which ran 14 times" and offers no park.
2. The user removes `grilling` from the skill page anyway. The dialog lists
   the skills in this install that call it, 4 because
   `improve-codebase-architecture` is left out, and says `grill-me` does
   nothing without it. The list and count come from the demo install.
3. Packs → `mattpocock/skills` → Map. `grilling` lights its users;
   `ask-matt` wears a badge for the 3 skills it lists that are not
   installed. Show for Codex: the mentions of
   `setup-matt-pocock-skills` show its Codex state. Then pstack's map:
   `poteto-mode`'s `/deslop` step points at a plugin that is not installed.

**CLI and MCP beat:** an agent asked to "clean up my skills" calls
`skill_links` first and answers: "`grilling` looks unused, but `grill-me`
runs it. Keep it." The same words as the dialog.

**Homepage:** a sixth scenario in `packages/marketing/src/home/jobs-scenarios.ts`,
"What breaks if I delete this?". The pain side is the real manual work
(grep every skills folder for `grilling`, follow each `../` path by hand,
repeat per agent). The fix side is
`npx skill-studio@latest links grilling`. The stat line, "10 of 12 popular
packs link their skills", links to the evidence file. The "Skill links"
section of the walkthrough draws the `mattpocock/skills` map with the shared
layout code from the same snapshot.

## 9. Slices

Each slice is one vertical change, under about 500 lines.

1. Links (PR #798): the real-pack snapshots, core `skill_links`, `skill-studio
   links`, the `skill_links` MCP tool, a `docs/flows.md` row, and layers 1, 2
   and 4 of section 10. Layer 5 runs before the per-agent rule is relied on
   by a warning.
2. The preview of section 4 (`breaks`, `through`, `unmet`) for remove,
   park, turn-off and pack remove: CLI `--check`, MCP `check: true`, and the
   desktop dialogs. One e2e spec: removing `grilling` lists the skills that
   run it in the demo install, and Keep changes nothing. Layer 2 gains these
   actions and the full property P2.
3. Subset install: the Store and pack add show `unmet` and offer to add it,
   with the plan reading fetched upstream text.
4. The skill page Links card, Home's Missing links group, the Not used in
   30 days guard, and `MissingLink` in `doctor`.
5. The pack map, with the four layout tests and one e2e spec (select
   `grilling`, its users light up; Show for Codex, the users of
   `setup-matt-pocock-skills` show its Codex state).
6. The homepage scenario and the walkthrough section.

Later, outside the first delivery: editor warnings and typo suggestions,
the pack author check (flow H), marking outside changes, and previews for
fork, split, move and update.

## 10. Outcome and how we prove it

### 10.1 The outcome

On every machine Skill Studio supports, for every agent it supports, however
the skills got there:

1. Before a remove, park, turn-off or pack remove, the user sees every skill
   that would stop working, per agent, each with the line that says so. Index
   entries and mentions never raise an alert.
2. The answer matches what the agent itself does. A link reported OK is one
   the agent can open or run; a link reported Missing is one it cannot.
3. Desktop, CLI and MCP give the same answer for the same setup.
4. Where Skill Studio cannot tell, it says "not known", never "fine".

The developer's own machine is one scenario among many (a dotagents port
with a `matt-` prefix, `~/.claude/skills` linked as a whole to
`~/.agents/skills`). No check depends on it, and no check reads it.

### 10.2 The scenarios

Each dimension below comes from what Skill Studio already supports
(`crates/skill-studio-core/src/harness.rs`, `identity.rs`,
`docs/agent-skill-conventions.md`, `docs/capabilities.md`).

| Dimension        | Values                                                                                                                                                                                                                                                                                                                                                              |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Agent            | Claude Code, Codex, OpenCode, pi, Cursor, Grok Build. Claude Code reads only its own folders; the other five also read `~/.agents/skills`. Cursor also reads the Claude Code and Codex folders; OpenCode and Grok Build also read Claude Code's, global and project. Agent settings can hide a skill: a Codex `skills.config` entry, an OpenCode permission.        |
| How it got there | skills.sh (`npx skills`, global and project lock); dotagents (named, wildcard, wildcard with `exclude`); a Claude Code plugin (pstack from `cursor/plugins`, listed as `pstack:…`); a Codex plugin; a port that renames (`sergical/pstack-skills`); Skill Studio Copy and fork; a folder made by hand; a Cursor or Grok Build plugin; pi standalone Markdown skills |
| Folder layout    | Shared folder only; per-skill links from an agent folder to the shared one; the whole agent folder linked to the shared one; separate copies per agent; a mix of these                                                                                                                                                                                              |
| Scope            | Global; project; both, with the project copy shadowing the global one                                                                                                                                                                                                                                                                                               |
| State            | Parked; turned off for one agent; installed under another name; left out of a wildcard; two copies that differ, only one with the link                                                                                                                                                                                                                              |
| Home location    | Default; `CLAUDE_CONFIG_DIR`, `PI_CODING_AGENT_DIR` or `GROK_HOME` set; the Codex and OpenCode home overrides the host resolves (`crates/skill-studio-host/src/discovery.rs`)                                                                                                                                                                                       |
| Surface          | Desktop, CLI, MCP                                                                                                                                                                                                                                                                                                                                                   |
| Platform         | macOS on Apple silicon and Intel (what ships). Core and host tests also run on Linux in CI. Windows is not shipped and not covered.                                                                                                                                                                                                                                 |

### 10.3 How each scenario is proven

Seven layers, cheapest first. Each has an oracle that is not the code under
test: the thing that decides whether the answer is right.

| Layer                                  | Proves                                                                                         | Oracle                                                                                                                                                                                                                                                                            | Runs                                                                                  |
| -------------------------------------- | ---------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| 1. Real-pack corpus                    | Detection on real text                                                                         | A list of links a person checked line by line, kept next to each snapshot                                                                                                                                                                                                         | CI, Linux and macOS                                                                   |
| 2. Property test over action sequences | Every combination of starting point, actions, agents, layouts and scopes reachable by real use | Three parts that share no logic: a real temp home changed the way real tools change it; an oracle that asks only the operating system whether each agent can open each path and list each name; and Skill Studio's own scan and plan. The rules of section 5.7 are the properties | Core and host tests, CI on macOS and Linux                                            |
| 3. Planted bugs                        | That layer 2 catches a failure it was never written for                                        | `cargo mutants` on `skill_links.rs` (already run on every PR for the core and host crates), plus a fixed list of named bug classes found while building, each of which layer 2 must catch                                                                                         | CI                                                                                    |
| 4. Real installers                     | The setups real tools make, read back through CLI and MCP                                      | The files the real `npx skills`, `dotagents` and `claude plugin install` write, from the snapshots served by the local git pack server                                                                                                                                            | `verify-skill-studio`, and the `skills-cli-contract` workflow on macOS                |
| 5. Real agents                         | That OK and Missing match what each agent does                                                 | A successful tool result in the agent's own transcript: `grilling` loaded after `grill-me`, `../choreograph/SKILL.md` read                                                                                                                                                        | By hand in a disposable home, before slice 2 merges and on each agent's major version |
| 6. UI flows                            | Dialogs, Home and the map show the core answer                                                 | The page as a user reads it, found by role and name                                                                                                                                                                                                                               | e2e on the harness, WebKit and Chromium                                               |
| 7. Upstream drift                      | That a pack changing its links is noticed before users hit it                                  | Layer 1 rerun on each pack's latest commit                                                                                                                                                                                                                                        | Weekly CI, report only                                                                |

Layer 2 does not list cases. It states what must hold after any action
and lets random real-world sequences look for a counterexample:

- **Properties, from section 5.7.** P1: for every link, agent and scope,
  the scan's state equals the oracle's. P2: each preview list equals what
  the oracle derives from the states before and after the action (rule 3).
  P3: after undoing a journaled action, the oracle sees what it saw before.
  P4: nothing but a real break raises an alert. P1 runs from slice 1; P2
  and P3 arrive with the previews in slice 2.
- **What the oracle can and cannot judge.** It opens paths and lists
  folders, so it judges path reads and folder visibility. It cannot judge
  invocation by name when an agent setting hides a skill, so generated
  setups do not change agent settings, and layer 5 judges those against
  real agents.
- **Inputs.** A starting point from section 5.7, a folder layout, and a
  random sequence of actions drawn from what a user or a tool can do next
  in that state: install from a real pack, remove, park, unpark, turn off
  or on for one agent, rename, edit a link in, delete outside Skill Studio,
  undo. Built with `proptest` strategies (already a core dev dependency), so
  a failure shrinks on its own to the shortest sequence that still fails.
- **The oracle stays dumb on purpose.** It knows nothing about parked
  folders, disabled folders, layouts or installers. It opens paths and
  lists folders, as an agent does. That is why it can catch a rule Skill
  Studio got wrong.

A prototype of this layer, run on 2026-11-27 against `mattpocock/skills`,
`emilkowalski/skills`, and pstack and `cursor-team-kit` from
`cursor/plugins`, on real temp folders with real links:

- 200 random sequences, 1,600 actions, all properties held for the final
  model (5 m 36 s).
- 8 planted bugs, one at a time, none with a check written for it: paths
  resolved by text instead of by the file system, Claude Code treated as
  reading the shared folder, parked skills counted as present, the disabled
  folder counted as visible, a renamed install matched by its old name,
  plans that forget the second agent, plans that ignore a whole-folder
  link, alerts on mentions. All 8 were caught, 5 in the first sequence and
  all within 9, each shrunk to a 1 or 2 step reproduction.
- It also found a bug in the prototype's own planner that no one had
  thought to test: on macOS a temp folder's real path starts with
  `/private/var` while the plan used `/var`, so predicted fixes did not
  match. The real build must compare real paths everywhere. That bug class
  joins layer 3's list.

Layer 5 runs each
agent headless on its cheapest model, with the target skill present and
absent, in each folder layout and with a hiding setting; a full pass costs
under a dollar. It counts a read or an invocation only from a successful
tool result for that path or skill name. A refused or failed call is a
completed result too, and the Claude Code usage reader records requests,
so neither counts as proof. Its table
of results, with agent versions, goes in `docs/verification/skill-links.md`.
The sibling-path probe below covers the tool path for two clients.

#### Sibling reads through a linked skill folder

Measured on macOS on November 28, 2026. A temporary project linked a skill
into each client's project skills directory. Different marker files occupied
the same sibling path next to the link and next to its target. Neither
client used an account or a remote model.

| Client and entry point                                    | Starting path reported by the client        | Successful read of `../sibling/proof.txt` |
| --------------------------------------------------------- | ------------------------------------------- | ----------------------------------------- |
| Claude Code 2.1.332, slash skill and native Read tool     | Project `.claude/skills/link-probe` symlink | Marker beside the link                    |
| Codex CLI 0.172.0, app-server skill list and command/exec | Physical `pack/link-probe/SKILL.md`         | Marker beside the target                  |

Claude received a scripted local model response requesting Read from the
base directory in the expanded skill. Codex ran `cat` from the listed
skill path. These results establish those tool paths. A shell command in
Claude or a model that chooses another path can behave differently.
An unmeasured client gets Not known when the two resolutions disagree.

### 10.4 Done means

- Layer 1: every hand-checked link found and no listed non-link reported,
  with the count per pack in the PR.
- Layer 2: the properties hold for the CI sample (seeded, seed printed) and
  for the weekly long run, with sequence counts and run time in the PR.
- Layer 3: no surviving mutant in `skill_links.rs` that changes a link
  state, and every named bug class caught by layer 2.
- Layer 4: one evidence folder per installer, with identical CLI and MCP
  output.
- Layer 5: a filled table of agent × layout × present or absent, every cell
  matching the spec's rule. A cell that does not match changes the rule.
- Layer 6: the e2e specs pass in both engines.
- No step reads the developer's home, projects or session history.

Rare cases not tested, per the testing skill:

- Names built by a rule. Marked "may name skills by rule"; no test.
- Non-English prose names. Paths, slashes and backticks do not depend on
  language and carry 73 of Lark's 97 links.

## 11. Open questions

1. **Index rule.** Two rules on 12 packs found the three index skills and
   rejected the false ones. A pack author could also mark an index in
   `metadata` (the Agent Skills spec has no dependency or index field).
   Propose it upstream after slice 1 shows the heuristic's misses.
