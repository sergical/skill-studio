# Check skill usage

You'll know which skills show up in the last 30 days of local history Skill Studio can read.

You need readable agent history and installed skills to match against it. A skill counts as used when Skill Studio finds evidence that the agent loaded it. A skill can have zero recorded uses when sessions are missing or history was deleted.

## Desktop

Open **Home** and look at **Recently used** or **Not used in the last 30 days**. Open **Sessions** to inspect recent sessions and see the evidence by agent.

| Agent       | Local evidence                                                                                                      |
| ----------- | ------------------------------------------------------------------------------------------------------------------- |
| Claude Code | Project transcripts and supported subagent transcripts. Can show use and a listing when the transcript includes it. |
| Codex       | Session and archived-session rollouts. Can show use and a listing when the rollout includes it.                     |
| Cursor      | Agent transcripts for local projects. Use evidence only.                                                            |
| OpenCode    | Its local SQLite database. Use evidence only.                                                                       |
| pi          | Local session transcripts. Use evidence only.                                                                       |
| Grok Build  | Completed sessions with an update log and a summary. Use evidence only.                                             |

## CLI

```sh
npx -y skill-studio@latest usage
```

A fixture with two skills and no session history reported:

```text
2 of 2 skills not used in 30 days
```

## MCP

Call `skill_usage` for the same usage window, or `skill_sessions` to inspect recent sessions. The fixture tool call returned `data.days: 30` and listed three fixture skill names under `data.unused`. Look for partial results if a history source couldn't be read.

These reports leave skills and sessions alone. If you expected a use that isn't shown, check that the agent is enabled in Skill Studio and that its session file still exists. Claude Code deletes transcripts after 30 days by default. [Claude Code settings](https://code.claude.com/docs/en/settings)
