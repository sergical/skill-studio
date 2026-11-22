# Troubleshooting

You'll be able to tell whether a failure came from scanning, installing or starting the MCP server.

## A skill is missing

Open **Skills** and check the selected scope. Include the agent and project discovery in the desktop settings. Make sure the skill folder is readable and contains `SKILL.md`. From the CLI, run `scan` and include the folder with `--project` if discovery missed it. In MCP, call `scan` and check the scope it reports. See [See every skill](/docs/inventory/).

## An install or update fails

Check [the requirements for that job](/docs/requirements/). For a Git install, confirm the repo and ref exist and your Git credentials can read them. For a skill update check, confirm `gh` is signed in and available to the process running Skill Studio. Read the failed step before trying again.

## The MCP server won't start

Verify `npx -y skill-studio@latest --version` from the terminal. Check that your client can find `npx`, then restart it and try `diagnose` again.

If the launcher says it couldn't find the platform binary, npm may have cached the launcher without its matching binary. Use the path from the error or the client's log to find the affected `~/.npm/_npx/<hash>` folder. Move that one folder out of `_npx`, then run the command again to fetch both packages. When it prints the version, restart the client. [MCP startup after a release](https://github.com/sergical/skill-studio/issues/759)

## Too many open files in 0.4.1

With many skills, the desktop app can run out of open-file slots and fail to load Model listing, Sessions or Packs. Check the local log at `~/Library/Application Support/com.skillstudio.app/timing.jsonl` for `Too many open files`. A fix is in progress. Use the matching CLI or MCP read in the meantime. [Machine requirements and open-file limits](https://github.com/sergical/skill-studio/blob/main/docs/capabilities.md#macos-open-file-limit)

## Diagnose exits with code 1

The scan ran and found issues. Read the **Issues** section or `data.issues` in JSON. A healthy run can say `No issues.` The scan leaves your skill files alone. Use [Clean up and undo](/docs/cleanup/) if you want to act on a finding.
