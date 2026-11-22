# CLI

Use the Skill Studio CLI to scan, check, and manage skills from the terminal. Supported restore actions appear in the same Activity history as changes from the app.

## Run it

The CLI needs Node.js and macOS. Run it with `npx`. You do not need to install it first. `@latest` makes `npx` get the newest version, not a copy it saved before.

```sh
npx -y skill-studio@latest diagnose
```

The command prints the skills it finds and any problems. On a clean Mac, it ends with "No issues."

To see all commands:

```sh
npx skill-studio@latest --help
```

## Check your skills

List skills in supported agents' home and project folders, with the path and ID of each copy:

```sh
npx skill-studio@latest scan
```

Find problems, for example broken links or bad frontmatter:

```sh
npx -y skill-studio@latest diagnose
```

Find skills that have copies with different content:

```sh
npx skill-studio@latest conflicts
```

## Check skill listing limits

Check how much space your agent has for skill names and descriptions:

```sh
npx skill-studio@latest doctor
```

When the list exceeds its budget, agents may shorten or drop descriptions. Parking skills you do not use frees space in the list. Skills can also be hidden by design, for example because they are manual-only or belong to another project.

On one Mac with 438 installed skills, Skill Studio 0.4.1 measured these listings:

| Agent       | Full list size and budget              | What fit                                 |
| ----------- | -------------------------------------- | ---------------------------------------- |
| Claude Code | 70,845 of 75,000 characters            | All 152 descriptions fit.                |
| Codex       | 20,734 tokens needed, 10,000 available | Codex shortened 124 of 165 descriptions. |

Claude Code and Codex record their skill list for each session. Skill Studio shows a current estimate for the other agents.

## Find unused skills

Show which skills your agents used in the last 30 days of available history. Skills with no recorded use in that window come first.

```sh
npx skill-studio@latest usage
```

To use a different number of days:

```sh
npx skill-studio@latest usage --days 7
```

## Fix a skill

```sh
npx skill-studio@latest fix --skill my-skill
```

The CLI repairs what it can. If it cannot fix a problem, it prints the path to the file, so you can fix it yourself.

## Park and unpark

Park a skill to hide it from every agent. The CLI does not delete it. You can park a skill that is in the shared `.agents/skills` folder.

```sh
npx skill-studio@latest park my-skill
```

To use the skill again, or to undo a park:

```sh
npx skill-studio@latest unpark my-skill
```

If more than one copy has the same name, the CLI lists each copy with its path and id. Run the command again with the id:

```sh
npx skill-studio@latest park --id <id>
```

## Turn a skill off

Turn a skill off for all agents with `disable`:

```sh
npx skill-studio@latest disable my-skill
```

To turn it on again:

```sh
npx skill-studio@latest enable my-skill
```

`disable` and `enable` are the same as `park` and `unpark`.

To turn a skill off for one agent only, check first, then confirm:

```sh
npx skill-studio@latest turn-off my-skill --agent codex --check
npx skill-studio@latest turn-off my-skill --agent codex --confirm
```

This splits a shared skill into one copy for each agent, then parks the copy of the agent you name. After this, `npx skills update` does not update these copies.

## Undo

Use `undo` to try the latest change. It supports installs, updates, removes, and repairs. Use `unpark` to reverse a park and `park` to reverse an unpark.

```sh
npx skill-studio@latest undo
```

To undo an older change, list the history, then restore one event. Restore can refuse after later edits or when a backup has expired.

```sh
npx skill-studio@latest events
npx skill-studio@latest restore --event-id <id>
```

## Keep skills current

Find skills that have a newer version:

```sh
npx skill-studio@latest outdated
```

Update a skill that you installed from skills.sh:

```sh
npx skill-studio@latest update --skill my-skill --method skills-sh
```

Pack updates apply to copies with no local edits that are managed by a supported installer. Check an installed pack for changes with `pack show owner/repo --remote`. Run `pack update owner/repo` to apply the updates.

## Add and remove

Install a skill from a GitHub repository:

```sh
npx skill-studio@latest add owner/repo --name my-skill
```

Remove a skill managed by skills.sh or dotagents, or a copy or fork tracked by Skill Studio:

```sh
npx skill-studio@latest remove my-skill
```

For a supported removal, `undo` restores the skill while its backup is available.

To share skills as a pack, run `npx skill-studio@latest pack export ./my-pack --skill my-skill`. The new folder contains the selected skills and install instructions.

## Use it in scripts

Add `--json` to get output that a script can read:

```sh
npx skill-studio@latest diagnose --json
```

Commands such as `diagnose`, `conflicts`, `fix` and `doctor` exit with code 1 when they complete with findings. A batch update also exits with code 1 if every item fails. If only some items fail, it exits with code 4. Check `data.errors` for the batch update errors.
