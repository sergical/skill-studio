# Install a pack

You'll have the skills you chose from a Git repository in your home or project.

Have Node and `npx` for the installer and Git access to the source. For a private repository, use credentials that work with that Git URL.

## Desktop

In **Packs**, select **Add from a repo**. Enter the repo, select the skills and agents, pick Global or a project, then install with skills.sh. Return to **Packs** and open the source row to check the skills. For a pack you already have, **Check upstream** can offer **Add N new**.

## CLI

This example pins the public fixture pack to a tag. Replace the source, ref and agent with your own. Add `--project-path` with a folder for a project install. Review the first result before you run the second command.

```sh
npx -y skill-studio@latest pack add sergical/skill-studio-fixtures --ref v1.0.0 --agent claude-code --dry-run
npx -y skill-studio@latest pack add sergical/skill-studio-fixtures --ref v1.0.0 --agent claude-code
npx -y skill-studio@latest pack list
```

```text
Installed 3 skills; 0 skipped; 0 failed.
```

## MCP

Call `pack_add` with `source`, `ref_pin` and `harnesses`. Set `dry_run` to true for the preview, then false to install. Omit `skills` to list and install the missing skills in the repo, or pass the names you want. Use `pack_list` to check the result.

Each installed skill returns its own undo event ID. Restore it from **Activity**, use CLI `restore --event-id`, or call `restore_event`. If the preview can't read the repo, check the ref and Git credentials. If install fails, check the step's error and [installation requirements](/docs/requirements/).
