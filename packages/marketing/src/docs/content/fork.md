# Fork and pull upstream

You'll have an editable local fork of a skill and a way to bring upstream changes into it.

Forking and pulling upstream here are desktop-only. Use an eligible shared Global GitHub skill with no independent agent copies. You'll need signed-in `gh`, access to the repository and `tar`. Keep Node and `npx` available for the original installer.

1. On **Home**, select **Check now** in **Updates** so the app can resolve the upstream skill.
2. Open the skill and choose **Fork**. Check that the skill shows its fork source and opens for editing.
3. When it has an upstream update, choose **Pull latest**. Review the files it changed. If both sides changed the same file, resolve the conflict markers and save the file.
4. Back up any edits you want to keep before choosing **Un-fork**. Confirm that the skill is reinstalled from its original source.

If Fork is missing, check that the skill is the shared Global copy and can be traced to a GitHub source. If the app asks you to check first, check `gh` sign-in and run **Check now** again.

## Switch a pack's source

The CLI's `pack switch-source` and MCP's `pack_switch_source` move a skills.sh pack to an existing Git source, such as a fork you already host. This is available in the CLI and MCP. It installs from the new source and remembers the original for comparison.

Use the source shown by `pack list` or `pack_list` and the new Git URL as the destination. Preview with `--dry-run` or `dry_run: true`, then apply and verify the source in the pack list. Restore each changed skill with the event ID if you need to switch it back. Local edits are skipped.
