# Real-shaped scan profile

Release CLI scan, seed 1, 378 skills, ten process iterations.
Profiler: samply 0.13.1, 1,000 Hz. Apple M3 Max, 36 GiB RAM, macOS 26.7.
The read budget was set to 60 seconds to capture a complete walk.
The first attempt used the default budget and returned a partial inventory.

The table counts leaf frames. These are sampled wall-time stacks, not CPU percentages.
Unresolved system frames remain in the denominator.

| Leaf frame                          | Samples | Share |
| ----------------------------------- | ------: | ----: |
| `lstat`                             |    2144 | 28.5% |
| `__open_nocancel`                   |     940 | 12.5% |
| `__open`                            |     897 | 11.9% |
| `__getdirentries64`                 |     516 |  6.9% |
| `libsystem_malloc.dylib unresolved` |     476 |  6.3% |

Total nonempty stacks: 7521.

The flame graph data are in `real-shaped-profile.json.gz`. The symbol sidecar
is `real-shaped-profile.json.syms.json`. Load both with samply to inspect the stacks.
Most samples stop in filesystem calls. A cache could reduce these calls, but
its invalidation rules need proof. This profile alone does not justify a cache
or a change in the scan lease. No hot-spot fix is included in this baseline.

Command after fixture generation:

```sh
samply record --save-only --unstable-presymbolicate --iteration-count 10 \
  -o docs/perf/real-shaped-profile.json.gz \
  "$CARGO_TARGET_DIR/release/skill-studio" scan --fixture "$FIXTURE_HOME" \
  --json --read-timeout-ms 60000 --project "$FIXTURE_PROJECT"
```

Repeat `--project` for each generated project. Set HOME and agent cache roots
to the fixture home, with telemetry off. The command must never select the real home.

Source commit: `c205022785ad40db6530dedc99b542d16630a326`.

Measured executable SHA-256: `71f1906eaa8c7fc50d2c8a5c5604fef2cc76a82b9c57fe106de95c7eee90d90e`.

The measured executable was copied out of the shared Cargo target folder.
