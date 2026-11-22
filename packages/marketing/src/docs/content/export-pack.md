# Export a pack

You'll have a folder of skills and a README with an install line.

Choose live, non-plugin skills and a new or empty destination you can write to. Export works locally. To share the pack, publish the folder to a Git repo you control.

## Desktop

Select skills on **Home** or **Skills**, then choose **Pack** in the selection bar. Choose the destination and review the plan. Enter the repo if you know where you'll publish it. Select **Export**, then check the folder.

## CLI

Use an absolute path. Preview, then run again without `--dry-run` to write the files.

```sh
npx -y skill-studio@latest pack export /Users/example/example-pack --skill fixture-greeting --dry-run
npx -y skill-studio@latest pack export /Users/example/example-pack --skill fixture-greeting
```

```text
Wrote 1 skill and a README to /Users/example/example-pack.
```

## MCP

Call `pack_export` with `dir`, `skills` and `dry_run: true`. Repeat with `dry_run: false` to export. The optional `source` field fills in the repository in the README. Use `project_path` to export from a project.

Check that the folder contains `README.md` and one `skills/<name>/SKILL.md` for each skill. Use the README's install line after you publish. If the destination already has files, pick a new folder. To reverse a local export, delete the exported folder. No history event is created.
