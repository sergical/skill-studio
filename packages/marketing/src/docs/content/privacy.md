# Privacy and telemetry

You'll know what Skill Studio reads, what goes online and how to turn telemetry off.

## Local reads

Skill Studio reads skill folders, local install records, agent settings and project and session records. It uses them to find skills and projects, check which agents can load a skill, and report use. The desktop app keeps a local timing log for diagnostics.

## Network requests

- The Skill Store uses Skill Studio's proxy to browse and search skills.sh, or goes directly to skills.sh if configured with a key.
- Installs, updates, forks and pulls can contact npm and the skill source. The pack upstream check uses Git. The installed-skill update check uses GitHub through `gh`.
- Public install-count lookups use GitHub and skills.sh. The app's own update check uses GitHub Releases.
- An MCP client can pass tool results, including local skill names and paths, to its model. The client's data settings apply to that exchange.

## Turn off desktop telemetry

On the welcome screen, **Send crash reports, timings, and screen performance to Skill Studio** starts on. Switch it off before you select **Continue**. Later, open **Settings → Telemetry** and turn off **Send crash and performance data**. The switch shows the saved setting. Turn it on again in Settings to resume sending reports.

With telemetry enabled, the app sends reports to Sentry. Crash and app-interface error reports carry the app release, error type, code location and scrubbed stack frames. Rust crash reports can also include OS, CPU and runtime details. Performance reports carry timestamps and durations for named app operations, commands and views, along with step timings, status and error codes. Background refresh timings are sampled.

The reports leave out skill names and contents, local skill and project paths, session transcripts, user and machine names, command arguments, and free-form error text. Skill Studio sends code filenames and function names to locate errors. The CLI and MCP server also read the saved telemetry setting. For a CLI run or MCP process, set `SKILL_STUDIO_TELEMETRY=0` in its environment to turn sending off.
