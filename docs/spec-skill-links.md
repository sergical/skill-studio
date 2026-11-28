# Skill links

Status: draft for review. Evidence:
[how skills in popular packs depend on each other](research/skill-links-evidence.md).

## 1. The need

**Before I remove, park, turn off or skip a skill, show me what stops
working.**

Skills name other skills. Ten of twelve popular packs checked on
2026-11-27 have links between their skills. Three shapes recur:

- A **wrapper**: `grill-me` (1.6M installs) is one line, `Call the Skill
  tool with "grilling".` Without `grilling` it does nothing.
- A **base skill**: 23 pstack skills open `../pstack-harness/SKILL.md`; 24
  Lark skills open `../lark-shared/SKILL.md`. Parking the base breaks most of
  the pack.
- An **index**: `ask-matt` and `ask-emil` list their whole pack. Removing a
  listed skill leaves a dead menu entry; removing the index breaks nothing.

No agent and no installer shows this today. Skill Studio already knows every
copy of every skill, for every agent, in every scope, so it is the one place
that can answer the question for the user's own machine, whatever its layout.

## 2. What a link is

A link is a line in skill A's folder that names skill B. Each link keeps the
file, the line number and the line itself, so every surface can show why it
thinks A needs B.

### 2.1 Strength

Strength decides where a link is shown. It is decided per line.

| Strength      | Meaning                                       | Found by                                                                                                   | Shown                                       |
| ------------- | --------------------------------------------- | ---------------------------------------------------------------------------------------------------------- | ------------------------------------------- |
| **Needs**     | A reads B's files or loads B before it starts | A relative path that resolves into B's folder; "load", "read", "first" before the name                     | Dialogs, skill page, map, `doctor`          |
| **Hands off** | A runs B for a step, or sends the user to it  | `/b`, `pack:b`, the Skill tool with `"b"`, "invoke", "run", "use the b skill"                              | Dialogs, skill page, map                    |
| **Mentions**  | A names B as related, optional, or see-also   | The name in backticks, bold, a link label, or "the b skill" with no stronger word; "if available"          | Skill page and map, on demand               |
| **Lists**     | A is an index and B is one of its entries     | A line in a list item or table row of an index skill (rule below), unless the line is a Needs or Hands off | A badge on the index, never a line or alert |

An **index** names at least 5 skills and at least half of its pack, and at
least half of those lines are list items or table rows. On the checked
packs this finds `ask-matt`, `ask-emil`, `pstack-poteto-help` and
`pstack-poteto-mode`, and rejects `executing-plans` (a workflow that hands
off from prose).

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

Measured on 40 random links after these rules: 37 point at the right skill
in the right direction. The detection is a heuristic, which is why the line
always travels with the link.

### 2.3 Targets

| Target           | Example                                                          | What the user sees                                   |
| ---------------- | ---------------------------------------------------------------- | ---------------------------------------------------- |
| Installed skill  | `grilling` in the same scope                                     | Its name; the link state from section 3              |
| Missing, in pack | `ask-matt` names `pr`, which the pack ships but is left out      | "Not installed" and Add, through `pack_add`          |
| Missing, unknown | A name no installed skill or known pack has                      | "Not found". Mentions of unknown names are not shown |
| Another pack     | `pstack-harness` names `emil-unslop-code`                        | The skill and its pack; "Not installed" if absent    |
| Not a skill      | A subagent (`pstack-poteto-agent`), a plugin command (`/deslop`) | The kind and the name, never a state                 |

The pack a missing name belongs to comes from the pack's own listing
(`pack show --remote`), never a guess from the name.

## 3. Per agent, per machine

The same pack gives different answers on different machines. A link is
resolved for each agent that has a copy of A, using that agent's folders, so
the answer fits the machine it runs on.

- **Needs (path)**: resolve the path from the folder the agent reads A from.
  On a machine where `~/.claude/skills` is a link to `~/.agents/skills`,
  every `../` path resolves for Claude Code. On a machine with per-skill
  copies in `~/.claude/skills`, `../pstack-harness/SKILL.md` resolves only
  if `pstack-harness` is copied there too. Project skills resolve inside the
  project's folder.
- **Hands off (by name)**: B must be in the agent's own list: a copy it
  reads, not parked, not turned off for it.
- **Renamed installs**: a copy installed under another name than its
  upstream name still answers to both. If A asks for `grilling` by name and
  the copy is installed as `matt-grilling`, the link is **Renamed**: the
  agent will not find it by the name A uses.

Link states, worst first. A skill's card and row show its worst state:

1. **Missing**: no copy for this agent.
2. **Renamed**: a copy exists under another name, and A asks by name.
3. **Off**: parked, or turned off for this agent.
4. **Elsewhere**: a copy exists in another agent's or another scope's
   folder only.
5. **OK**.

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
  (`from`, `to`, `strength`, `target_kind`, `state` per agent, `evidence`
  with file, line and text) and a summary (`skills`, `links`, `missing`,
  `not_known`).
- **Plan reports**. The dry-run plans of `remove`, `park`, turning a skill
  off for an agent, `pack remove`, and `pack add` with a subset gain a
  `breaks` list: the Needs and Hands off links that the plan would turn
  Missing or Off, direct and indirect (section 6.3). CLI, MCP and desktop
  show the same list because it comes from the same plan.
- **Doctor**. One new issue kind, `MissingLink`, for a Needs or Hands off
  link in state Missing or Renamed. It shows in `doctor`, on Home, and in
  `skill-studio doctor --json`.
- **Cost**. The checked packs hold 15 MB of Markdown across 316 skills, 5.2
  MB for the largest pack. Links are computed per folder and kept by the
  folder's existing content hash, so a scan rereads only changed folders.
  Names are matched in one pass per file over all names in scope
  (`memchr`, already a core dependency; `aho-corasick` is in `Cargo.lock`
  through `regex` but would be a new direct dependency). Budget: within the
  scan budgets of `docs/spec-headless-performance-observability.md`.
- **Errors**. Follow D7 in `docs/spec-core-primitives.md`. An unreadable
  file is a `not_known` reason on that skill, not an error for the call.

## 5. Surfaces

Every surface reads the same core result. A link never shows without its
line one click away.

### 5.1 Before a change (first slice)

The remove, park, turn-off-for-agent and pack remove dialogs gain a
"What stops working" section from the plan's `breaks`:

> **Remove `grilling`?**
> 6 skills run it: `grill-me` (does nothing without it), `grill-with-docs`,
> `improve-codebase-architecture`, `triage`, `wayfinder`, `loop-me`.
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
  that need it: "Used through `grill-me`, 14 times."

### 5.4 Pack map

Section 6.

### 5.5 CLI

```
skill-studio links [<skill>] [--pack <source>] [--agent <id>] [--missing]
                   [--project-path <path>] [--json]
```

```
$ skill-studio links grilling
grilling  (mattpocock/skills)
Used by 6 skills:
  grill-me                        hands off  SKILL.md:7   Call the Skill tool with "grilling".
  grill-with-docs                 hands off  SKILL.md:7   Call the Skill tool twice, for "grilling" and "domain-modeling".
  ...
Listed by: ask-matt
Removing it breaks: grill-me (directly), 5 others (a step each)
```

`remove`, `park` and `pack remove` print the plan's `breaks` in their dry
run and before they ask to confirm.

### 5.6 MCP

One tool, `skill_links`, with the same input and output as the CLI's
`--json`. Its description tells the agent to call it before it removes,
parks or turns off a skill, and to show the user what would break. The plan
tools already return `breaks`.

## 6. The pack map

### 6.1 What we take from T3 Code's Environments map

T3 Code's map (Settings → Environments,
`apps/web/src/components/settings/environmentsMap.logic.ts` and
`EnvironmentsMap.tsx` in `pingdotgg/t3code`, read on 2026-11-27) shows
machines, the routes that reach them and the links between them. What makes
it good is how it thinks, not how it looks. Each row below is one decision,
why it works there, and what it becomes here.

| T3 Code decision                                                                                                                                        | Why it works                                                                | Skill Studio                                                                                                                                                                                                           |
| ------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| The graph is built from what the client knows. It never probes, and a list it could not read is "not known", never "no links".                          | The map never lies about a machine it cannot see.                           | Links come from local text. An unreadable plugin or an unchecked upstream says "links not known".                                                                                                                      |
| One node per machine, however often it is named.                                                                                                        | A machine is one thing to the user.                                         | One node per skill per scope. Its copies across agents are one node with a state per agent.                                                                                                                            |
| Edge states are ranked worst first, and a card wears the worst.                                                                                         | The eye goes to what needs attention.                                       | Link states from section 3, worst first.                                                                                                                                                                               |
| Two links between the same machines at the same access are drawn as one line with two arrowheads.                                                       | Half the lines, same meaning.                                               | Two skills that name each other at the same strength (siblings) are one line.                                                                                                                                          |
| Many machines with the exact same incoming links become one stack: "a thousand VMs under two hosts cost one node and two lines".                        | The map stays readable at any size.                                         | Skills with the exact same set of skills linking to them become one stack (up to 5 skills share one set in `mattpocock/skills`, `pstack-skills` and `azure-skills`). Unlinked skills fold into one chip past 12 nodes. |
| Focus lights one hop, because a link does not chain: A linked to B linked to C gives A nothing on C.                                                    | It draws only what is true.                                                 | The same honesty gives the opposite rule: a Needs or Hands off link does chain. If C goes, B fails, so A's step fails. Focus lights direct links strong and indirect ones faint.                                       |
| Actions on the map (drag a plus onto a machine to link) use the same rule as the dialogs.                                                               | One rule, so the map and the dialog never disagree.                         | Remove, park and Add from the map open the same plan dialogs with the same `breaks`.                                                                                                                                   |
| Columns that mean something: this client, its environments, machines known only through links.                                                          | Position carries meaning before the user reads a label.                     | Columns by role: index badge on top, then **Start here** (skills nothing in the pack names), **Steps**, **Base** (skills named by several and naming few).                                                             |
| A shape key: the layout changes only when membership, names or wiring change, never on a status change.                                                 | The map does not jump while the user reads it.                              | The same: a state change recolors, a link change relays out.                                                                                                                                                           |
| A status line with two numbers, a legend ("How to read the lines"), and a help popover of four rules.                                                   | The user learns the map in one look.                                        | "39 skills · 43 links" for `mattpocock/skills` (index entries are not links), plus "· N missing" when any are. Legend: solid = needs, arrow = hands off, dotted = mentions.                                            |
| Search with a match count, arrow keys between columns, Esc clears the selection before it closes Settings, zoom and fit.                                | Keyboard users get the whole map; Esc never loses work.                     | The same, with `/` to search.                                                                                                                                                                                          |
| Every node is a real button over an `aria-hidden` SVG of lines, and the details panel lists every member of a stack.                                    | The drawing is decoration; the buttons and the list are the accessible map. | The same, and the Links card (5.2) is the list view of the selected skill.                                                                                                                                             |
| Layout is pure and tested: nothing overlaps, the same map whatever order data arrives in, 1000 machines as a few stacks, legible from 560 to 1400 wide. | Those are the rules a user notices when broken.                             | The same four tests, on the fixture packs of section 7.                                                                                                                                                                |

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
its own `LICENSE` file, in one shared folder (`fixtures/real-packs/`) that
the core tests, the desktop harness, the CLI checks and the marketing site
all read:

| Pack                     | Commit    | License, holder                                          | Markdown | Why it is in                                  |
| ------------------------ | --------- | -------------------------------------------------------- | -------: | --------------------------------------------- |
| `mattpocock/skills`      | `bbadaa6` | MIT, Matt Pocock                                         |  0.29 MB | Wrapper (`grill-me`), base skills, an index   |
| `sergical/pstack-skills` | `dd839d3` | MIT, Lauren Tan (generated from `cursor/plugins` pstack) |  0.32 MB | One base skill read by 23 others, two indexes |
| `obra/superpowers`       | `22f0b46` | MIT, Jesse Vincent                                       |  0.35 MB | `pack:name` hand-offs                         |
| `emilkowalski/skills`    | `64eb3de` | MIT, Emil Kowalski                                       |  0.36 MB | Path reads, siblings, an index that lists all |

- Only the skill folders' text files are kept. A README in the folder names
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
| Renamed        | A prefixed copy asked for by its upstream name                            | Renamed                                                                         |
| Across packs   | A link from one pack's skill to another pack's skill                      | Target pack named                                                               |
| Agent layouts  | The same pack as a whole-folder link, per-skill links, per-agent copies   | Path links resolve per layout; turning off for one agent breaks only that agent |
| Project scope  | The base skill in the project, its users global                           | Elsewhere                                                                       |

**Desktop harness** (`apps/desktop/src/dev/harness`): the fixture estate
installs the real packs, set up the way real people install them:

- `mattpocock/skills` without `pr`, `teach` and
  `improve-codebase-architecture`, the way a dotagents `exclude` leaves them
  out. `ask-matt` and `setup-matt-pocock-skills` still name them, so the map
  shows real dead entries and one real Missing hand-off.
- `sergical/pstack-skills` with `pstack-harness` turned off for Codex, so
  Show for Codex lights the 23 skills that read it.
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

1. Home lists `grilling` as not started in 30 days. The row says "Used
   through `grill-me`, 14 times" and offers no park.
2. The user removes `grilling` from the skill page anyway. The dialog lists
   the 6 skills that run it, and says `grill-me` does nothing without it.
3. Packs → `mattpocock/skills` → Map. `grilling` lights its 6 users;
   `ask-matt` wears a badge for the 3 skills it lists that are not
   installed. Then `sergical/pstack-skills` → Show for Codex: the 23 skills
   that read `pstack-harness` turn red in Codex only.

**CLI and MCP beat:** an agent asked to "clean up my skills" calls
`skill_links` first and answers: "`grilling` looks unused, but `grill-me`
runs it. Keep it." The same words as the dialog.

**Homepage:** a sixth scenario in `packages/marketing/src/home/jobs-scenarios.ts`,
"What breaks if I delete this?". The pain side is the real manual work
(grep every skills folder for `grilling`, follow each `../pstack-harness`
path by hand, repeat per agent). The fix side is
`npx skill-studio@latest links grilling`. The stat line, "10 of 12 popular
packs link their skills", links to the evidence file. The "Skill links"
section of the walkthrough draws the `mattpocock/skills` map with the shared
layout code from the same snapshot.

## 9. Slices

Each slice is one vertical change, under about 500 lines.

1. The real-pack snapshots, core `skill_links` with the fixtures of
   section 7, `skill-studio links`, the `skill_links` MCP tool, a
   `flow_parity.rs` row, and a `docs/flows.md` row.
2. `breaks` in the remove, park, turn-off and pack remove plans; the dialogs
   show it; the CLI dry runs print it. One e2e spec: removing `grilling`
   lists its 6 users, and Keep changes nothing.
3. The skill page Links card; Home's Missing links group; the
   Not used in 30 days guard; `MissingLink` in `doctor`.
4. Pack add and the Add Skill sheet offer what picked skills need.
5. The pack map, with the four layout tests and one e2e spec (select
   `grilling`, its users light up; Show for Codex on pstack, the users of
   `pstack-harness` show broken).
6. The homepage scenario and the walkthrough section.

## 10. Tests, per the testing skill

One test per user flow at the level the user sees it, and one rule test:

- **Rule**: links are the same whatever order the scan reads folders in.
- **Flows**: the e2e specs in slices 2 and 5; the CLI/MCP parity row in
  slice 1.
- **Real packs**: one corpus test over the four snapshots, checking the
  links a person checked by hand: `grill-me` hands off to `grilling`, 6
  skills run `grilling`, 23 pstack skills need `pstack-harness`, `ask-matt`
  and `ask-emil` are indexes and raise no alerts.
- **Shapes**: one table test over the core fixtures of section 7, each row
  checking what the user would see (the `breaks` list or its absence).

Rare cases not tested, per the testing skill:

- Names built by a rule. Marked "may name skills by rule"; no test.
- Non-English prose names. Paths, slashes and backticks do not depend on
  language and carry 73 of Lark's 97 links.

## 11. Open questions

1. **`..` through a link.** When `~/.claude/skills/a` is a link to
   `~/.agents/skills/a`, does the agent resolve `../b` against the link's
   folder or the target's? The operating system uses the target's; a tool
   that cleans the path first uses the link's. Check with Claude Code and
   Codex in the `verify-skill-studio` home before slice 1 ships the
   per-layout rule.
2. **Index rule.** Two rules on 12 packs found the four index skills and
   rejected the false ones. A pack author could also mark an index in
   `metadata` (the Agent Skills spec has no dependency or index field).
   Propose it upstream after slice 1 shows the heuristic's misses.
