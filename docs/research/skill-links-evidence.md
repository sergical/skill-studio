# How skills in popular packs depend on each other

Checked November 27, 2026. The user question is **"If I remove, park or skip
this skill, what stops working?"** This file holds the evidence behind
[the skill links spec](../spec-skill-links.md). It reads only public pack
repositories, cloned at the commits below. No one's installed skills, session
history or project files were read for these numbers.

## Method

For each pack, every folder with a `SKILL.md` is a skill (a `SKILL.md` nested
inside another skill's folder is left out as an example or fixture). Every
text file in a skill's folder was searched, line by line, for the name of
every other skill in the same pack. A line counts only when the name appears
in one of these forms:

| Form                                         | Example from the packs                                          |
| -------------------------------------------- | --------------------------------------------------------------- |
| A path into the other skill's folder         | ``先阅读 [`../lark-shared/SKILL.md`](../lark-shared/SKILL.md)`` |
| `/name`                                      | `tell the user to run /setup-matt-pocock-skills`                |
| `pack:name`                                  | `Invoke the superpowers:writing-plans skill.`                   |
| The Skill tool with the name in quotes       | `Call the Skill tool with "grilling".`                          |
| The name in backticks, bold, or a link label | ``see the `code-review` skill``                                 |
| The name followed by "skill"                 | `the code-review skill`                                         |

A bare word was never counted. Names such as `teach`, `research` and `pdf`
match ordinary prose.

A relative path is resolved from the file it is written in, and counts only
when it lands in another skill's folder. A first run matched paths by text
and was wrong for `remotion-best-practices`: that skill carries its own copy
of each sibling's guide (`remotion-best-practices/remotion-studio/REFERENCE.md`),
so its `../remotion-studio/REFERENCE.md` stays inside its own folder. For the
same reason, a name written inside a bundled copy (a subfolder named after
the sibling) is not counted.

A skill is treated as an index (a menu of the pack) when it names at least 5
skills and at least half of its pack, and at least half of those lines are
list items or table rows. Its lines count as "lists", not as dependencies. A
count alone also flagged `executing-plans`, a 323-line workflow that hands
off to 9 skills from prose, and `remotion-markup`.

## Results

| Pack (commit)                                                                                                         | Skills | Links | Linked / standalone | Index skills  | Most needed (skills that name it, not counting indexes) |
| --------------------------------------------------------------------------------------------------------------------- | -----: | ----: | ------------------: | ------------- | ------------------------------------------------------- |
| [cursor/plugins, `pstack/`](https://github.com/cursor/plugins/tree/b7b93c36be9bcc36a74db438e0fd385d660bc559/pstack)   |     59 |    99 |             48 / 11 | `poteto-help` | `poteto-mode` 7, `unslop` 7                             |
| [microsoft/azure-skills](https://github.com/microsoft/azure-skills/tree/7847020998043c6231929b364c35393b7cdab270)     |     51 |   146 |              48 / 3 | none          | `azure-prepare` 13                                      |
| [mattpocock/skills](https://github.com/mattpocock/skills/tree/bbadaa631c37af594e5a05c9cefacd9956136b18)               |     39 |    69 |              30 / 9 | `ask-matt`    | `grilling` 7                                            |
| [larksuite/cli](https://github.com/larksuite/cli/tree/c5fbf1a52df77d5cb9cac2b1501770b2fb9af7d2)                       |     34 |    97 |              26 / 8 | none          | `lark-shared` 24                                        |
| [heygen-com/hyperframes](https://github.com/heygen-com/hyperframes/tree/b6cfb496b3476e26d2b9171df5e6e611d78e83ae)     |     23 |    52 |              22 / 1 | none          | `hyperframes` 17                                        |
| [juliusbrussee/caveman](https://github.com/juliusbrussee/caveman/tree/e74a11ccdb38b98d15c9f9545787c907dd15f0ab)       |     22 |    18 |             12 / 10 | none          | `caveman` 5                                             |
| [anthropics/skills](https://github.com/anthropics/skills/tree/e4bb5d960d87126ff3b3a8af1a104b7db67560ba)               |     20 |     5 |              6 / 14 | none          | `pdf` 2 (not local, see below)                          |
| [emilkowalski/skills](https://github.com/emilkowalski/skills/tree/64eb3deb9a5fbfa041faf783e0bc3fd635872e9c)           |     20 |    55 |              20 / 0 | `ask-emil`    | `review-animations` 8                                   |
| [obra/superpowers](https://github.com/obra/superpowers/tree/22f0b463e7a5e579904dd6211a520321035526f7)                 |     15 |    30 |              13 / 2 | none          | `using-git-worktrees` 4                                 |
| [remotion-dev/skills](https://github.com/remotion-dev/skills/tree/a3f4098a2afd0e13f6a6f8c8760e275f75457ea9)           |     14 |    23 |              14 / 0 | none          | `remotion-markup` 4                                     |
| [prisma/skills](https://github.com/prisma/skills/tree/b4bebffd0d0c3499c3d342f69380886a28e28880)                       |     11 |    20 |               9 / 2 | none          | `prisma-orm-setup` 5                                    |
| [vercel-labs/agent-skills](https://github.com/vercel-labs/agent-skills/tree/5a37a6329fbad3666c3207392a6a7c0110e92efb) |      9 |     0 |               0 / 9 | none          | none                                                    |

Ten of the twelve packs have links between their skills. Only
`vercel-labs/agent-skills` is fully standalone. In `anthropics/skills`,
`claude-api` names `pdf`, `xlsx` and `pptx` as the API's pre-built skills,
not as the local skills of the same name, so that pack is standalone in
practice too.

`leonxlnx/taste-skill` (one skill, no links) was also checked on
November 27 and is left out of the table.

## Three findings that shape the design

**A popular skill can be a one-line wrapper.** `grill-me` is #6 on the
[skills.sh leaderboard](https://skills.sh) with 1.6M installs. Its whole body
is `Call the Skill tool with "grilling".` `grilling` has 1.2M installs
([pack page](https://skills.sh/mattpocock/skills)). The counts are totals, not
per user, so they cannot say how many installs miss `grilling`, only that
`grill-me` was installed about 400K more times than the skill it runs.

**Several packs have one base skill that many skills read.** 24 Lark skills
read `../lark-shared/SKILL.md`. Six Matt Pocock skills send the user to
`/setup-matt-pocock-skills` when the tracker is not set up. 17 HeyGen skills name `hyperframes`, though some
of those lines mean the CLI of the same name. Removing or parking the base
skill breaks most of the pack at once.

**The links are not all the same strength.** In the sample below, the
"backticks or bold" form covered strong loads (`**Load first,
non-negotiable:** motion-doctrine`), optional ones (`If cost-analysis is
available`), and siblings (`the counterpart to find-animation-opportunities`).
The form alone cannot tell them apart; the words around the name can.

**A port can add links the original does not have.** pstack is a Cursor
plugin in `cursor/plugins`, also listed on skills.sh under that repo. Its
skills name a skill in another plugin of the same repo: `poteto-mode` says
"Before commit → `/deslop` from the `cursor-team-kit` plugin". A port for
other agents,
[sergical/pstack-skills](https://github.com/sergical/pstack-skills/tree/dd839d3de2bae4805b7ecef09b6584633293dfd5)
(58 skills, generated from the original), renames every skill with a
`pstack-` prefix and adds a `pstack-harness` skill that 23 of its skills read
first. The same pack, installed two ways, has a different base skill and
different names to resolve.

## Accuracy

After the path fix, 40 links were drawn at random (seed 7) across all packs
and each was read by hand against the line that produced it. This sample was
drawn while the pstack row was still the port, so its pstack links name
`pstack-` skills.

- 37 of 40 point at the right skill, in the right direction. The 3 wrong
  ones: a line that says not to call `lark-approval`, and two lines that name
  a CLI that shares its name with a skill (`hyperframes`, `caveman`).
- Of the 37 right ones, 6 are optional ("if available", "see also") or
  sibling mentions that would not break anything if the target were gone.
- One right link got the wrong kind: `pstack-poteto-help` reads
  `../pstack-harness/SKILL.md`, but as an index all its lines counted as
  "lists". The kind has to be decided per line, not per skill.

The sample before the path fix (same seed, different links) also had one
back reference: a child skill saying it is "called by `azure-app-onboard`",
which points the wrong way.

Two kinds of names are not found by this method: names built by a rule
(pstack's "the **principle-<title>** skill, with the title in kebab case")
and plain-prose names in languages other than English. The path form does
not depend on language, so 73 of Lark's 97 links (all paths) are found
anyway.

## One machine, for contrast

On the machine where this was checked, the installed set already had links
to skills that are not there. Its `agents.toml` leaves out `matt-pr`,
`matt-teach` and `matt-improve-codebase-architecture`, but `ask-matt` and
`matt-setup-matt-pocock-skills` still send the agent to them. Its
`pstack-harness` points at `emil-unslop-code`, which comes from another pack
and is not installed. That machine links `~/.claude/skills` as a whole to
`~/.agents/skills`, so every `../` path resolves for Claude Code there. A
machine with per-skill copies in `~/.claude/skills` would not get the same
answer, which is why the spec resolves links per agent folder.
