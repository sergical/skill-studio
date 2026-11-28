# Skill Studio desktop design rules

Skill Studio answers three questions about a person's skills: did an agent use
it, did the agent see its description, and is anything broken. Every page
answers one of them, in the same words and the same shapes as the others.
Read this before you add a page, a card or a string.

## Words

Use these words in every string a person reads. Code identifiers may keep
`harness`, `client` and `listing`; UI text may not.

| Say                                                        | Never say                             | Means                                                  |
| ---------------------------------------------------------- | ------------------------------------- | ------------------------------------------------------ |
| agent                                                      | client, harness                       | Claude Code, Codex, OpenCode, pi, Cursor, Grok Build   |
| use, used                                                  | load, loaded, invocation (as a count) | the model ran a skill in a recorded session            |
| skill list                                                 | listing, allocation                   | the names and descriptions an agent puts in the prompt |
| What it sees                                               | visibility, configuration             | an agent's skill list for one scope                    |
| What it used                                               | sessions report                       | an agent's recorded sessions                           |
| budget                                                     | allocation                            | the size limit of the skill list                       |
| Full description, Shortened, Name only, Not listed, Hidden | Described, Omitted, Truncated         | a skill's state in a skill list                        |
| Global, Project                                            | global scope                          | where a skill is installed                             |
| session                                                    | transcript, capture                   | one recorded conversation with an agent                |
| Broken, Warnings, Updates                                  | Has issues, problems                  | what needs the user, always in this order              |
| Needs attention                                            | issues                                | Broken, Warnings and Updates together                  |

"Loads" keeps one meaning: an agent accepts a SKILL.md file ("Claude Code still
loads it"). It never counts uses.

The state labels live in
[`listing-words.ts`](../../packages/lib/src/listing-words.ts). Import
`LISTING_STATE_LABEL`, `SESSION_LISTED_LABEL` or `ATTENTION_LABEL`; never write
a local map. Three local maps had drifted to three names for one state.

Broken means an agent does not load the skill. Which category a skill is in
comes from [`skill-attention.ts`](../../packages/lib/src/skill-attention.ts),
so Home's counts, the Skills groups and filter, and the command palette list
the same skills in the same order.

Known gap: notes the core writes (`crates/skill-studio-core/src/skill_sessions.rs`,
`listing_visibility.rs`) still say "listing". The CLI and MCP print the same
notes next to a command named `listing`, so they change with the CLI's words,
in their own change.

Agent names come from `HARNESS_CATALOG` or `deploymentLabelFromAgentId`. Never
type a list of agent names into copy: the next agent added would be missing.

## Headlines name the consequence

A summary card's number says what the person gets, not the input that caused
it. "0 of 35 skills keep a description", not "300 characters". The input goes
in the line under it ("Budget 300 characters · 0.01% of context").

## Page anatomy

- Every page is a `PageShell`. Its title is the sidebar label. Tabs within a
  place are line `Tabs` in `toolbar` (see `AgentTabs`), with the first label
  on the page's left edge; filters go in `actions`.
- Every titled part of a page is a `PageSection`
  ([PageSection.tsx](src/components/Shell/PageSection.tsx)): the title on the
  page's left edge, its controls or a note on the right, content 12px below.
  Never write a section heading by hand; a hand-made one lands on its own edge
  and its own gap.
- A report opens with one `SummaryCard` and one `SummaryMetric`
  ([SummaryCard.tsx](src/components/ui/SummaryCard.tsx)). The card's title is
  a `PageSection` title outside the box, so it lines up with the sections
  below; only the answer sits in the card.
- Space says what belongs together: 12px from a title to its content and
  between items of one section, 32px between sections (`gap-8` on the page's
  section column). Equal gaps everywhere erase the grouping.
- A trailing ghost button on a title row or a row's right edge takes `-mr-2.5`
  (`-mr-2` for `px-2`), so its text, not its padding, ends on the right edge.
- Section titles are `text-heading font-semibold`. A summary number is
  `text-display`. Everything else is `text-body`, `text-small` or
  `text-caption`. Do not add `text-heading-lg`, `text-title` or arbitrary
  sizes to a report page: each extra size is one more level to parse.
- A `<details>` gets its chevron from `App.css`. Do not add
  `list-none` or a marker of your own unless the summary is a row with its own
  chevron, and then set `data-own-marker`.

## Places

Home, Skills, Plugins, Packs, Agents, Activity, then Parked. Agents has two
tabs: What it sees (`kind: "listing"`) and What it used
(`kind: "sessions"`); both keep the `harness` in the view, so a tab switch
keeps the chosen agent. Home links to the page that answers in full: the week
opens Activity, an agent opens its What it sees tab, a count opens Skills
filtered to exactly those skills. Home's counts and its prompt come from the
history of the agents the user keeps, so they agree with that filter.
The command palette opens on Needs attention, in the Skills table's order; its
Go to list shows the same places as the sidebar.

## Color

- `accent` is for things a person can press or has selected: buttons, focus
  rings, toggles.
- `data` (`--color-data`) is for counts drawn as shapes: Home's bars and the
  Activity heatmap. It shares the accent hue today; use the `data` name so the
  two jobs can split in one place.
- `error`, `warning` and `success` mean broken, drifted and done. Never use
  them for decoration.

## Charts

- Our users are developers and read charts fluently. A chart explains its
  encodings with a legend of marks (a swatch, a ring, a ramp) and one or two
  words each, and labels its axes with units. Never a sentence caption such as
  "Area: tokens · Brightness: uses".
- Every mark is reachable: hover or focus shows the skill, click opens it.

## Missing data

An agent that is turned on but has no records Skill Studio could read says
"No readable history", never "0 uses", "No uses" or "unused". "No uses" is
only for an agent whose history was read and holds no use of that skill. An
agent with no budget estimate says "No estimate", never "no limit".

## Action weight

One row leads with one action. The action that takes skills away (Remove)
is `ghost`, the one that keeps them current (Update) is `outline`. Anything
destructive opens a plan or a confirmation before it runs. A row with
nothing to do shows its actions on hover or keyboard focus; Update shows
outright, as `outline`, once a check finds the item behind.
