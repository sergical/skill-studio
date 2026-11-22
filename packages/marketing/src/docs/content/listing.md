# Check listing budgets

You'll see which skill descriptions fit in each agent's estimated model listing.

A description budget limits how much text an agent includes in the list of skills shown to the model. Skill Studio knows budgets for Claude Code and Codex. For Cursor, OpenCode, pi and Grok Build, it shows no known limit. Estimates depend on your installed skills, settings and any recent listing evidence the app can read.

## Desktop

Open **Model listing**. Pick an agent and scope, then check the entries:

| State            | What the model's list gets                                        |
| ---------------- | ----------------------------------------------------------------- |
| Described (full) | The complete description.                                         |
| Shortened        | Part of the description.                                          |
| Name only        | A name without the description.                                   |
| Omitted          | No entry. The budget ran out.                                     |
| Hidden           | Excluded by design, for example a parked skill. This is expected. |

## CLI

```sh
npx -y skill-studio@latest listing
```

One fixture row read:

```text
claude-code global: budget 8000 characters, full listing 33 characters. Described 1, truncated 0, name only 0, omitted 0, hidden 0.
```

## MCP

Call `listing` and read the per-agent totals and states in `data.listings`. The fixture call returned each agent's `budget`, `budget_unit` and `entries` there. To see what a past session recorded, call `skill_sessions` and check for Claude Code or Codex listing evidence in that session.

On a Mac with 438 skills, Claude Code fit all 152 listed descriptions in 70,845 of its 75,000 characters. Codex needed about 20,734 tokens against a 10,000-token budget and shortened 124 of 165 descriptions.

The report changes no settings. When a current estimate and a past session differ, check their scope, settings and available history. Use **Show hidden skills** if you're looking for an intentionally hidden entry.
