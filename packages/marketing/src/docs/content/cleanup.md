# Clean up and undo

You'll have fixed, parked or removed a skill and know how to recover it.

You need write access to change files. Removing a managed skill needs its installer. For dotagents, parking and unparking also need dotagents.

## Desktop

1. Open **Home** and review **Broken** and **Warnings**. For a problem in the opening YAML block of a skill's `SKILL.md`, open the skill and review the repair shown in its banner before applying it.
2. To take a skill out of use, select **Park**. Find the parked skill and select **Unpark** to put it back.
3. To remove a supported managed install, copy or fork, use **Remove** on its skill page. Read the confirmation.
4. To undo a supported install, update, remove or repair, open **Activity → History** and select **Restore** on the matching change.

## CLI

Start with `npx -y skill-studio@latest diagnose`. For an opening YAML repair, copy the deployment ID from `scan` and use it in place of `DEPLOYMENT_ID`. Use `jq` to extract the preview data. Check the diff in the file before you apply it.

```sh
npx -y skill-studio@latest preview-repair --deployment-id DEPLOYMENT_ID --json | jq .data > repair.json
npx -y skill-studio@latest apply-repair --preview-json repair.json
```

For a single unambiguous skill name, the following commands show the park and remove paths. The second command reverses the first. The last command reverses the remove if it is the latest change.

```sh
npx -y skill-studio@latest park fixture-greeting
npx -y skill-studio@latest unpark fixture-greeting
npx -y skill-studio@latest remove fixture-greeting
npx -y skill-studio@latest undo
```

For an older change, run `npx -y skill-studio@latest events`. Pass the matching ID to `npx -y skill-studio@latest restore --event-id EVENT_ID`.

## MCP

Use `diagnose`, then `preview_frontmatter_repair` with the deployment ID. After review, pass the unchanged `data` object as `preview` and choose an allowed `mode` in `apply_frontmatter_repair`. Use `park` or `unpark` with `{"skills":["fixture-greeting"]}`. Use `remove` with the deployment ID. Find the event in `list_events` and pass its ID as `event_id` to `restore_event`.

## Check the result

Run the scan again. A repair should clear its finding. A parked skill should be shown as parked. A removed skill should leave the installed list, then reappear after restore. In the fixture run, unpark printed `Turned on again.` followed by the restored path.

If a name matches multiple copies, take the ID from `scan` and pass `--id` to park or unpark, or `deployment_id` to the MCP tool. History restore can refuse if later edits changed the target or if its backup expired. Skill Studio keeps the newest 20 completed backups per target and backups younger than 30 days.
