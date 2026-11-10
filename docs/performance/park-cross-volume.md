# Cross-volume Park timing

Hardware: Apple M3 Max. OS: macOS-26.7-arm64-arm-64bit-Mach-O. Release CLI; 10 samples per size.

Each sample uses a fresh temporary HOME and a project on a separate 64 MiB APFS disk image. Each skill has SKILL.md, README.md, and scripts/run.sh. Input modes are 0750 for the skill directory, 0710 for scripts, and 0751 for the executable. The sample times the whole batch of CLI operations. It includes startup, scans, history, verified copy, and sync. p95 uses nearest rank, so it is the maximum of 10 samples.

| Skills | Flow   | Before median ms | After median ms | Median change | Before p95 ms | After p95 ms |
| ------ | ------ | ---------------: | --------------: | ------------: | ------------: | -----------: |
| 1      | park   |           120.38 |          132.14 |         +9.8% |        541.06 |       532.68 |
| 1      | unpark |            70.71 |           72.07 |         +1.9% |         74.16 |       109.50 |
| 100    | park   |         12992.72 |        14439.19 |        +11.1% |      14860.22 |     19209.43 |
| 100    | unpark |          7517.50 |        10432.23 |        +38.8% |       9723.80 |     17676.71 |

Before, the directories returned as 0755. After, both directory modes and the executable mode match the input. Every measured round trip also checked file bytes. No wall-clock assertion runs in CI.

Other developer builds were active during these runs. The values measure the combined cost of full sync, mode probing, mode preservation, and the whole CLI flow. They do not isolate the syscall cost. The p95 values are sensitive to that load and the cold first sample.

Raw samples and executable SHA-256 hashes are in park-cross-volume-before.json and park-cross-volume-after.json. Both measured builds use main 4c15eabb, after #468 and before the later #490 dependency updates. This holds dependencies equal for the sync comparison. The PR is now rebased on newer main; these samples describe that fixed-base experiment. The added direct nix dependency uses a version already in the main lockfile. No package version changes are in this fix.

The median changes exceed 10% for the 100-skill batch, so a samply 0.13.1 profile was recorded with --save-only and symbol sidecar output. Separate profiles sample Park and Turn on of one skill in the 100-skill fixture. The profiles include copy, directory sync, scans, history, process startup, and git calls.

To repeat with an existing second volume:

```sh
python3 tools/perf/park-volume-timing.py /path/to/release/skill-studio /path/to/second-volume output.json --runs 10
```

Profiles: `park-cross-volume-profile.json.gz` and
`park-cross-volume-unpark-profile.json.gz`, with symbol sidecars. These samples
show the complete CLI flow. They do not isolate F_FULLFSYNC latency.
