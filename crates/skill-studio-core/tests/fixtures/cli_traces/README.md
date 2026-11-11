# Recorded update and remove

`skills_sh_update_global.trace.json` and
`skills_sh_remove_global_claude_code.trace.json` were captured from
`npx skills@1.7.0` on 10 November 2026. Both calls returned status 0.

The capture used a temporary HOME, a private npm cache, pinned agent config
folders, and an empty XDG_STATE_HOME. A HOME-local Git URL rewrite mapped the
fixture source URL to a local repository. Git used fixed author and commit dates.
No user account or real home folder was used.

The initial command installed `trace-skill` for Claude Code and Codex:

```sh
npx skills@1.7.0 add https://github.com/skill-studio-trace/fixture.git --skill trace-skill --agent claude-code codex --yes --global
npx skills@1.7.0 update trace-skill --global
npx skills@1.7.0 remove trace-skill --yes --global
```

Commit `82d2bc49b57b14e190245b850dc8a79f1c0ae185` contains the v1 body and
`old.txt`. Commit `e15379a83d691954b418f8e661dc6b5016d2974d` changes the body,
removes `old.txt`, and adds `new.txt`. The JSON records complete skill files,
permission bits, raw agent link targets, and complete before/after lock data.
`$HOME` replaces only the temporary capture path. Timestamps remain recorded.

The replay tests compare exact command arguments and core bookkeeping around
these effects. They do not fetch from the network or test today's upstream CLI.
Other files in this folder predate these two recordings.
