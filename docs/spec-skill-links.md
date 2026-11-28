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
packs this finds `ask-matt`, `ask-emil` and pstack's `poteto-help`, and
rejects `executing-plans` (a workflow that hands off from prose).

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
  reads, not parked, not turned off for it. A plugin install answers to its
  plugin's prefix too: Claude Code lists pstack's skills as
  `pstack:poteto-mode`.
- **Renamed installs**: a copy installed under another name than its
  upstream name still answers to both. If A asks for `grilling` by name and
  the copy is installed as `matt-grilling`, the link is **Renamed**: the
  agent will not find it by the name A uses. A port that rewrites the names
  in its own text (as `sergical/pstack-skills` does) resolves cleanly.

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
  through `regex` but would be a new direct dependency). The work rides the
  existing `ScanMemo` (`crates/skill-studio-core/src/scan_memo.rs`), which
  already cut a no-change desktop rescan of 529 skills from 1,350 to 350 ms
  by not rereading unchanged folders (`docs/perf/README.md`). Proposed
  budgets, to be measured with `pnpm run perf:baseline` on the real-shaped
  profile plus the real-pack snapshots before slice 1 merges:
  - No-change rescan: zero extra file reads for links.
  - First scan: links add at most 10% to scan time.
  - Dialogs: no disk work at click time. `breaks` comes from links already
    in memory and from the checks the dialogs already run (`park_check`,
    `turn_off_check`), so the 100 ms p95 input target in
    `docs/spec-headless-performance-observability.md` still holds.
  - Store: no extra network. The SKILL.md the detail panel already fetches
    is the only text read before install.
  - Map: layout runs only when the map opens, and again only when its shape
    key changes. The largest checked pack has 59 skills.
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
skill-studio links [<skill>] [--pack <source> | --path <dir>] [--agent <id>]
                   [--missing] [--project-path <path>] [--json]
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

### 5.7 Every starting point, every action

**The model.** A link's state (OK, Missing, Renamed, Off, Elsewhere, Not
known) is derived per link and per agent from the files on disk. It is never
stored (rule 4 of `docs/state-machines.md`). Every action, whoever takes it
(Skill Studio, the skills CLI, dotagents, an agent, the user in Finder),
changes files, and the next scan derives the new states. So every flow comes
down to three questions: what the user sees before the action, after it,
and after undo.

**Five rules hold for every combination:**

1. **Before.** Every action Skill Studio runs shows, in its plan, the links
   it would break and the links it would fix, per agent, directly and
   through other skills. Nothing else.
2. **After.** The next scan shows exactly the links that changed state. A
   change made outside Skill Studio shows the same way, marked "changed
   outside Skill Studio".
3. **Predicted equals observed.** A plan's breaks and fixes equal the
   difference between the scan before and the scan after.
4. **Undo is exact.** Undo returns every link to its state before the
   action.
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

| Action                                                                                                    | On a skill others need                                                                                                                           | On a skill that needs others                                                                                                                                                                                            | Index or standalone                               |
| --------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------- |
| Install the whole pack                                                                                    | Clears any Missing rows it fixes                                                                                                                 | Offers only needs outside the pack (pstack's `/deslop`)                                                                                                                                                                 | Silent                                            |
| Install one skill or a subset                                                                             | "Fixes N links" when installed skills were missing it                                                                                            | Offers everything it needs, through other skills too, ticked for Needs and Hands off                                                                                                                                    | Silent                                            |
| Remove                                                                                                    | Lists every skill that stops working, per agent, direct and indirect; Keep or Remove anyway                                                      | Nothing breaks; silent                                                                                                                                                                                                  | Silent                                            |
| Park, unpark                                                                                              | As Remove, worded "until you unpark"; unpark lists what it fixes                                                                                 | Silent                                                                                                                                                                                                                  | Silent                                            |
| Turn off or on for one agent                                                                              | Lists what stops working in that agent only                                                                                                      | Silent                                                                                                                                                                                                                  | Silent                                            |
| Edit SKILL.md in the editor                                                                               | Changing its own `name:` makes every skill that asks for it by name Renamed; the editor says so before save                                      | A new line naming a missing skill is marked in the editor and goes Missing after save; a typo is "Not found" with the nearest installed name offered; deleting the only line that names a skill drops the link silently | Silent                                            |
| Edit outside Skill Studio                                                                                 | The same outcomes after the next scan, marked "changed outside"                                                                                  | Same                                                                                                                                                                                                                    | Silent                                            |
| Make an independent copy or fork                                                                          | Same name: nothing breaks. New name: every skill that asks for the old name goes Renamed, listed in the plan                                     | Keeps its links                                                                                                                                                                                                         | Silent                                            |
| Split                                                                                                     | Paths other skills read into the part that moves break; listed in the split plan                                                                 | Its own links move with the text that holds them                                                                                                                                                                        | Silent                                            |
| Move between project and global                                                                           | Skills that read it by path go Elsewhere; listed                                                                                                 | Its path reads into the old scope's skills go Elsewhere; listed                                                                                                                                                         | Silent                                            |
| Update one skill                                                                                          | If upstream renamed it, skills that ask by the old name go Renamed; listed in the update plan                                                    | "Now needs X" with Add, if upstream added a link; silent if it dropped one. Edited copies are skipped and keep their old links (existing rule)                                                                          | Silent                                            |
| Update the pack                                                                                           | As above for each skill                                                                                                                          | The skills CLI does not add new upstream skills on update, so a skill that now hands off to a new one goes Missing; the plan says so and offers Add                                                                     | Index lists new names as dead entries until added |
| Upstream deletes a skill                                                                                  | The local copy stays and nothing breaks. Check upstream says "Removed upstream; N of your skills still need it" so a cleanup does not break them | Silent                                                                                                                                                                                                                  | Silent                                            |
| Switch the pack to a fork                                                                                 | If the fork lacks a skill, the skills that need it go Missing; in the plan                                                                       | Same                                                                                                                                                                                                                    | Silent                                            |
| Remove the whole pack                                                                                     | Lists skills in other packs that need it (removing `cursor-team-kit` breaks pstack's `poteto-mode` step)                                         | Silent                                                                                                                                                                                                                  | Silent                                            |
| Remove outside Skill Studio (Finder, `npx skills remove`, a dotagents `exclude` then `dotagents install`) | No warning is possible before. After the scan: new Missing rows, marked "changed outside", with Add                                              | Silent                                                                                                                                                                                                                  | Silent                                            |
| A new agent appears (the user installs Codex)                                                             | Links are resolved for it; only its new breaks show                                                                                              | Same                                                                                                                                                                                                                    | Silent                                            |
| Undo any action above                                                                                     | Every link back to its earlier state; rows clear                                                                                                 | Same                                                                                                                                                                                                                    | Same                                              |

Each cell is the same five rules applied to that action. The worked
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
offers Add for Claude Code, which links the existing copy instead of
downloading again.

**B. Install from the CLI or an agent.**
`skill-studio pack add mattpocock/skills --skill grill-me --dry-run` lists
the plan and a `needs` line: "`grill-me` runs `grilling`, which is not
installed. Add it with `--skill grilling`." A real run without it installs
`grill-me`, prints the same line, and exits 0: a missing link is a warning,
not a failure. The MCP `pack_add` result carries the same `needs` list, and
the tool description tells the agent to ask the user before it adds more.

**C. A month later, tidying up.** Home's Not used in 30 days would list
`grilling` if nothing ran it directly. Because `grill-me` ran and needs it,
the row says "Used through `grill-me`, 14 times" and offers no park.

**D. Remove `grilling` anyway.** The skill page's Remove runs the plan. The
dialog: "6 skills run it: `grill-me` (does nothing without it),
`grill-with-docs`, `improve-codebase-architecture`, `triage`, `wayfinder`,
`loop-me`." Keep changes nothing. Remove anyway removes it, Home then shows
the 6 as Missing links, and Activity's Undo restores `grilling` and clears
them.

**E. Turn off `setup-matt-pocock-skills` for Codex.** `turn_off_check`
already runs before the dialog. The dialog adds: "6 skills send you to it.
In Codex they will point at nothing. Claude Code is not affected."

**F. Update a pack whose links changed.** Check upstream finds an update.
The update plan lists link changes next to file changes: "`to-spec` now
hands off to `codebase-design`, which you don't have," with Add in the same
dialog.

**G. Install pstack as a Claude Code plugin.** `poteto-mode` says to run
`/deslop` from `cursor-team-kit`. That plugin is not installed, so the Links
card shows Missing, in `cursor/plugins`. Plugins are installed by the agent,
so the row shows the agent's command to copy, as plugin packs already do.

**H. A pack author checks a pack before publishing.**
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
its own `LICENSE` file, in one shared folder (`fixtures/real-packs/`) that
the core tests, the desktop harness, the CLI checks and the marketing site
all read:

| Pack                             | Commit    | License, holder    | Markdown | Why it is in                                  |
| -------------------------------- | --------- | ------------------ | -------: | --------------------------------------------- |
| `mattpocock/skills`              | `bbadaa6` | MIT, Matt Pocock   |  0.29 MB | Wrapper (`grill-me`), base skills, an index   |
| `cursor/plugins`, `pstack/` only | `b7b93c3` | MIT, Lauren Tan    |  0.28 MB | An index, links across plugins (`/deslop`)    |
| `obra/superpowers`               | `22f0b46` | MIT, Jesse Vincent |  0.35 MB | `pack:name` hand-offs                         |
| `emilkowalski/skills`            | `64eb3de` | MIT, Emil Kowalski |  0.36 MB | Path reads, siblings, an index that lists all |

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
- `setup-matt-pocock-skills` turned off for Codex, so Show for Codex lights
  the 6 skills that send the user to it.
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

1. Home lists `grilling` as not started in 30 days. The row says "Used
   through `grill-me`, 14 times" and offers no park.
2. The user removes `grilling` from the skill page anyway. The dialog lists
   the 6 skills that run it, and says `grill-me` does nothing without it.
3. Packs → `mattpocock/skills` → Map. `grilling` lights its 6 users;
   `ask-matt` wears a badge for the 3 skills it lists that are not
   installed. Show for Codex: the 6 skills that need
   `setup-matt-pocock-skills` turn red in Codex only. Then pstack's map:
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

1. The real-pack snapshots, core `skill_links` with the fixtures of
   section 7, `skill-studio links`, the `skill_links` MCP tool, a
   `flow_parity.rs` row, a `docs/flows.md` row, a "Link state" row in
   `docs/state-machines.md`, and verification layers 1 to 5 of section 10.
   Layer 5 runs before this slice merges, because it decides the per-agent
   rule. Layer 3 grows with each later slice: a slice that adds a warning
   adds its action's rows.
2. `breaks` in the remove, park, turn-off and pack remove plans; the dialogs
   show it; the CLI dry runs print it. One e2e spec: removing `grilling`
   lists its 6 users, and Keep changes nothing.
3. The skill page Links card; Home's Missing links group; the
   Not used in 30 days guard; `MissingLink` in `doctor`.
4. Pack add and the Add Skill sheet offer what picked skills need.
5. The pack map, with the four layout tests and one e2e spec (select
   `grilling`, its users light up; Show for Codex, the users of
   `setup-matt-pocock-skills` show broken).
6. The homepage scenario and the walkthrough section.

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

| Dimension        | Values                                                                                                                                                                                                                                                                                                |
| ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Agent            | Claude Code, Codex, OpenCode, pi, Cursor, Grok Build. Claude Code reads only its own folders; the other five also read `~/.agents/skills`.                                                                                                                                                            |
| How it got there | skills.sh (`npx skills`, global and project lock); dotagents (named, wildcard, wildcard with `exclude`); a Claude Code plugin (pstack from `cursor/plugins`, listed as `pstack:…`); a Codex plugin; a port that renames (`sergical/pstack-skills`); Skill Studio Copy and fork; a folder made by hand |
| Folder layout    | Shared folder only; per-skill links from an agent folder to the shared one; the whole agent folder linked to the shared one; separate copies per agent; a mix of these                                                                                                                                |
| Scope            | Global; project; both, with the project copy shadowing the global one                                                                                                                                                                                                                                 |
| State            | Parked; turned off for one agent; installed under another name; left out of a wildcard; two copies that differ, only one with the link                                                                                                                                                                |
| Home location    | Default; `CLAUDE_CONFIG_DIR`, `PI_CODING_AGENT_DIR` or `GROK_HOME` set                                                                                                                                                                                                                                |
| Surface          | Desktop, CLI, MCP                                                                                                                                                                                                                                                                                     |
| Platform         | macOS on Apple silicon and Intel (what ships). Core and host tests also run on Linux in CI. Windows is not shipped and not covered.                                                                                                                                                                   |

### 10.3 How each scenario is proven

Seven layers, cheapest first. Each has an oracle that is not the code under
test: the thing that decides whether the answer is right.

| Layer                | Proves                                                                                  | Oracle                                                                                                                                                                                                                         | Runs                                                                                              |
| -------------------- | --------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------- |
| 1. Real-pack corpus  | Detection on real text                                                                  | A list of links a person checked line by line, kept next to each snapshot                                                                                                                                                      | CI, Linux and macOS                                                                               |
| 2. Resolution matrix | Resolution in every combination of agent, layout, owner, scope, state and home          | The file system: for Needs, opening the exact path the agent would open; for Hands off, the agent's list from the listing module, which has its own evidence against real agents (`docs/research/harness-listing-evidence.md`) | Host tests on real temp folders with real links, CI on macOS and Linux                            |
| 3. Action matrix     | Every action of section 5.7 from every starting point, on every skill of the real packs | Rules 3 and 4 of section 5.7: the plan made before the action against a fresh scan of the real files after it, and the scan after undo against the scan before. Rule 5 and the role columns of the table as named checks       | Core tests on the in-memory file system, and a sample on real temp folders, CI on macOS and Linux |
| 4. Real installers   | The setups real tools make, read back through CLI and MCP                               | The files the real `npx skills`, `dotagents` and `claude plugin install` write, from the snapshots served by the local git pack server                                                                                         | `verify-skill-studio`, and the `skills-cli-contract` workflow on macOS                            |
| 5. Real agents       | That OK and Missing match what each agent does                                          | The agent's own transcript, read with Skill Studio's session readers: did it load `grilling` after `grill-me`, did it open `../choreograph/SKILL.md`                                                                           | By hand in a disposable home, before slice 1 merges and on each agent's major version             |
| 6. UI flows          | Dialogs, Home and the map show the core answer                                          | The page as a user reads it, found by role and name                                                                                                                                                                            | e2e on the harness, WebKit and Chromium                                                           |
| 7. Upstream drift    | That a pack changing its links is noticed before users hit it                           | Layer 1 rerun on each pack's latest commit                                                                                                                                                                                     | Weekly CI, report only                                                                            |

Layer 2 is one property test: it builds every combination the agent
catalog allows, and prints any combination that fails.

Layer 3 is the permutation matrix. It runs every starting point × every
action × every skill of `mattpocock/skills` as the target, on the in-memory
file system: 10 × 18 × 39, about 7,000 cases. Agent, folder layout, scope,
home location and installer are covered pairwise, so every pair of their
values meets in at least one case, on real temp folders with real links. A
change outside Skill Studio is simulated by editing the files directly, as
Finder or another tool would. Each CI run also draws a seeded random sample
across all four packs and prints its seed; the weekly run takes the full
product. A failure prints the starting point, action, target, agent and
layout, which is a reproducible case, and the dev harness opens any case by
that id, so a failure can be seen in the UI as well as read. Budget: under 60 seconds per CI run,
measured with the first slice.

Layer 5 runs each
agent headless on its cheapest model, with the target skill present and
absent, in each folder layout; a full pass costs under a dollar. Its table
of results, with agent versions, goes in `docs/verification/skill-links.md`.
Layer 5 also settles open question 1.

### 10.4 Done means

- Layer 1: every hand-checked link found and no listed non-link reported,
  with the count per pack in the PR.
- Layer 2: every generated combination passes on macOS and Linux, with the
  combination count in the PR.
- Layer 3: every case passes rules 3, 4 and 5, with the case count and run
  time in the PR.
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

1. **`..` through a link.** When `~/.claude/skills/a` is a link to
   `~/.agents/skills/a`, does the agent resolve `../b` against the link's
   folder or the target's? The operating system uses the target's; a tool
   that cleans the path first uses the link's. Layer 5 answers it per agent.
2. **Index rule.** Two rules on 12 packs found the three index skills and
   rejected the false ones. A pack author could also mark an index in
   `metadata` (the Agent Skills spec has no dependency or index field).
   Propose it upstream after slice 1 shows the heuristic's misses.
