# Update skills and packs

You'll know which skills have changed upstream and will have applied the updates you chose.

**The installed-skill update check needs signed-in `gh`** with access to the source. The installer also needs Node, `npx` and source access. A pack's **Check upstream** uses Git to compare its recorded ref.

## Desktop

1. On **Home**, use **Check now** in **Updates**, then open a result and choose **Update**.
2. For a whole pack, open **Packs**, choose **Update** on its row and review the plan. Confirm and read the result for each skill.
3. If you edited a skill, read the overwrite warning. For an eligible copy, [fork it](/docs/fork/) to keep your edits.

## CLI

```sh
npx -y skill-studio@latest outdated
npx -y skill-studio@latest pack update sergical/skill-studio-fixtures --dry-run
npx -y skill-studio@latest pack update sergical/skill-studio-fixtures
```

```text
Would update 3 skills; 0 skipped.
Updated 3 skills; 0 skipped; 0 failed.
```

For one skill, name the skill and its recorded installer. For a skills.sh install, use:

```sh
npx -y skill-studio@latest update --skill fixture-greeting --method skills-sh
```

The fixture command reported `updated fixture-greeting` with an event ID. A pack pinned at a tag updates from that tag. The fixture run reapplied v1.0.0.

## MCP

Call `outdated`. To update a global skills.sh install, call `update` with `{"skill":"fixture-greeting","method":"skills_sh","scope":{"scope":"global"}}`. For a project, set the inner `scope` to `project` and add the absolute `project` path beside it.

For a pack, call `pack_update` with the source from `pack_list`. Set `dry_run` to preview what will update. Verify the finished result, including any skipped or failed steps.

Pack updates skip skills with local edits and plugin and npm packs. Use the plugin's own owner for plugin updates. If the check shows `unknown`, check `gh` sign-in and repository access. Restore an update from **Activity**, CLI `restore`, or `restore_event` with its event ID. A dotagents update can refresh multiple skills in the same scope under one event.
