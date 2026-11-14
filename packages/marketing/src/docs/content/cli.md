# CLI

The Skill Studio CLI does the same jobs as the app, from the terminal. It uses the same history, so you can undo a CLI change in the app.

## Run it

The CLI needs Node.js and macOS. Run it with `npx`. You do not need to install it first. `@latest` makes `npx` get the newest version, not a copy it saved before.

```sh
npx skill-studio@latest diagnose
```

To see all commands:

```sh
npx skill-studio@latest --help
```

## Check your skills

List every skill, with the path and id of each copy:

```sh
npx skill-studio@latest scan
```

Find problems, for example broken links or bad frontmatter:

```sh
npx skill-studio@latest diagnose
```

Find skills that have copies with different content:

```sh
npx skill-studio@latest conflicts
```

Check how much of each agent's skill listing budget your skills use:

```sh
npx skill-studio@latest doctor
```

## Find unused skills

Show which skills your agents used in the last 30 days, and which they did not. Unused skills come first.

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

Turning a skill off means parking it. This turns it off for all agents:

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

Undo the last change. If the last change was a park or an unpark, `undo` stops; use `unpark` or `park` instead.

```sh
npx skill-studio@latest undo
```

To undo an older change, list the history, then restore one event:

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

## Add and remove

Install a skill from a GitHub repository:

```sh
npx skill-studio@latest add owner/repo --name my-skill
```

Remove a skill that you installed with Skill Studio, `npx skills` or dotagents:

```sh
npx skill-studio@latest remove my-skill
```

To get the skill back, run `undo`.

## Use it in scripts

Add `--json` to get output that a script can read:

```sh
npx skill-studio@latest diagnose --json
```

`diagnose`, `conflicts`, `fix`, `doctor` and `update` exit with code 1 when they find a problem. Other errors use codes 2 and higher.
