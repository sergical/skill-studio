# Turn a skill on or off for one agent

You'll have one agent's copy off while the other agents can still load the skill.

Use a shared skill you can write to, or an independent copy for the agent you want to change. A shared copy is one folder read by several agents. Independent copies have their own folders.

## Desktop

Open the skill and find the agent in its locations. Turn that agent off and review the confirmation. For a shared skill, Skill Studio splits the skill into agent copies and parks the selected agent's copy. For an independent copy, park just that location. Check the other agents' locations are still active. Turn the parked location on again to keep the split, or restore the split event from **Activity** to get the shared layout back.

## CLI

For a shared skill, check before confirming. Replace the name and agent with yours.

```sh
npx -y skill-studio@latest turn-off example-review --agent codex --check
npx -y skill-studio@latest turn-off example-review --agent codex --confirm
```

The preview returned `"refusal": null` for the fixture. Confirm printed a line starting with `Parked example-review for codex.` Run `scan` to check the parked copy and the active ones. For a skill that already has separate copies, use `park --id` or `unpark --id` with the chosen copy's ID. Use `restore --event-id` to undo the split itself.

## MCP

Use `turn_off_check` with the shared `deployment_id` and `agent`. If the plan is right, pass the same values and `confirmed: true` to `turn_off_for_agent`. To change an independent copy, pass its `deployment_id` to `park` or `unpark`. For the shared layout, call `restore_event` with the split event ID.

The split refuses dotagents-managed skills, plugin skills and whole-folder links. Use the owning tool for managed skills. After a split, the original installer may update only the shared copy. Check the copies before assuming an upstream update reached each agent.
