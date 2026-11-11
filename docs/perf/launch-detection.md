# Desktop launch detection

**Provisional: recorded under load.** These samples include the real desktop
detection helper, Tauri async runtime initialization, login shell, project
discovery, probe-cache IO, and runtime cleanup. Each sample starts a fresh
native process. They do not measure WebView startup or paint time.

Source: `14da9726f33a354bc036415047a808a4098fa585`, clean worktree.
Hardware: Apple M3 Max, 38,654,705,664 bytes (36 GiB), macOS 26.7.
`DEVELOPER_DIR=/Library/Developer/CommandLineTools`. Release build, all features.

The FixtureBuilder home has 378 skills, 239 global, Claude Code and Codex
plugin caches, three broken SKILL.md files, and six native version stubs.
Saved settings keep only Claude Code. HOME, app data, projects, shell settings,
and agent executables point into the disposable fixture. The native example
checks the fixture marker and every detected executable path.

| Phase      | Metric                    |   N | Median ms |  p95 ms |  Max ms |
| ---------- | ------------------------- | --: | --------: | ------: | ------: |
| Cache miss | Detection helper          |  10 |   156.789 | 214.534 | 214.534 |
| Cache miss | Native process round trip |  10 |   166.796 | 224.463 | 224.463 |
| Cache hit  | Detection helper          |  10 |    25.760 |  33.749 |  33.749 |
| Cache hit  | Native process round trip |  10 |    37.488 |  45.408 |  45.408 |

The miss phase deletes the app-owned version-probe cache before each sample.
The hit phase keeps it across fresh processes. Executable trust and OS caches
are warmed before either phase. Both phases run the login shell once per
process. The callback timer includes runtime destruction and cache persistence.
The process timer also includes process launch and shutdown.

`launch-detection.json` contains all raw samples, nearest-rank p95, full source
SHA, binary SHA-256 hashes, hardware, load average, and Cargo/rustc counts.
Eleven Cargo processes were present throughout; rustc counts ranged from one
to five. This is a fixture result under load. It needs a quiet reference run
after wave 1. It is not a CI wall-clock limit.

Build `perf_detection_launch` from the desktop crate with `--release
--all-features`. Copy its executable out of the shared target folder. Build
and copy the core `perf_baseline` fixture generator from PR #477 and the host
`perf_detection` version fixture from PR #505. Then run:

```sh
python3 tools/perf-launch-detection.py \
  --binary /path/to/copied/perf_detection_launch \
  --version-binary /path/to/copied/perf_detection \
  --fixture-generator /path/to/copied/perf_baseline \
  --output docs/perf/launch-detection.json
```

Lead default, owner to confirm: run detection in the background on every
launch. Newly detected agents stay off until selected. Detection never writes
their folders. Settings saves only the app-owned agent choice.
