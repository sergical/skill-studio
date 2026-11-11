# Version probe deadlines and cache

**Provisional: recorded under load.** Raw samples in `probes.json` record the
load average and cargo/rustc process counts beside each elapsed time. The
source is `4c89efdb1218131702f5b03534a81834e5b330bd`, built in release mode.
Hardware: Apple M3 Max, 36 GiB, macOS 26.7. The JSON also records the binary
hash. These samples did not wait for other agents to stop building.

The fixture has six native version executables and empty agent config and
session roots. Every sample constructs and drops a runtime and spawner.
The timed interval includes cache loading, detection, and runtime cleanup.
This isolates version detection; it excludes the desktop login shell and UI.
The launch-detection report measures the shell separately.

| Metric           |   N | Median ms |  p95 ms |  Max ms |
| ---------------- | --: | --------: | ------: | ------: |
| uncached initial |  10 |    92.873 |  94.978 |  94.978 |
| uncached repeat  |  10 |    94.419 | 106.980 | 106.980 |
| persistent miss  |  10 |    95.633 |  98.468 |  98.468 |
| persistent hit   |  10 |     2.637 |   3.279 |   3.279 |

The control disables persistence in the same release binary. Its two phases
both execute every version command. Cache hits use a new spawner, so they
prove disk reuse across launches. A changed invocation path, resolved shim
target, size, exact mtime, file identity, change time, or effective PATH
invalidates a result. Failures and timeouts are
not cached. Desktop, CLI, and MCP runtime constructors load the cache under the selected
home at `.skill-studio/version-probes.json`. The file is bounded and mode 0600.

Repeat with disposable directories:

```sh
cargo build -p skill-studio-host --example perf_detection --release --all-features
"$CARGO_TARGET_DIR/release/examples/perf_detection" /tmp/skill-probes-control uncached
"$CARGO_TARGET_DIR/release/examples/perf_detection" /tmp/skill-probes-cached
```

## Profile

Generated recordings, symbol sidecars, and SVGs are not tracked. The folded
stacks, summaries, and `profile-metadata.json` retain the recorded measurements.
Counts include sleeping and are not CPU utilization percentages.
The cache-enabled recording includes ten misses followed by ten hits.

Regenerate SVGs from the recorded stacks with Inferno:

```sh
inferno-flamegraph < docs/perf/detection/uncached.folded.txt > docs/perf/detection/uncached-flamegraph.svg
inferno-flamegraph < docs/perf/detection/cached.folded.txt > docs/perf/detection/cached-flamegraph.svg
```

Record new profiles and symbol sidecars with Samply 0.13.1 after the release
build above. Use a new fixture directory for each recording:

```sh
samply record --save-only --unstable-presymbolicate -o docs/perf/detection/uncached.json.gz -- "$CARGO_TARGET_DIR/release/examples/perf_detection" "$(mktemp -d)/fixture" uncached
samply record --save-only --unstable-presymbolicate -o docs/perf/detection/cached.json.gz -- "$CARGO_TARGET_DIR/release/examples/perf_detection" "$(mktemp -d)/fixture"
```

| Leaf frame                           | Uncached samples | Cache-enabled samples |
| ------------------------------------ | ---------------: | --------------------: |
| `__semwait_signal`                   |              325 |                   139 |
| `__posix_spawn`                      |               22 |                    30 |
| `_kernelrpc_mach_vm_deallocate_trap` |               13 |        below top five |
| `readlink`                           |               12 |                     8 |
| `stat`                               |                8 |        below top five |
| `__rename`                           |   below top five |                    13 |
| `__open`                             |   below top five |                     7 |

Waiting for child exit dominates the uncached path. Reusing a successful
version avoids that work. The miss profile also shows cache renames: this
version rewrites the file after each successful agent probe. Batching those
writes is a separate performance change with its own measurements.

Timeout and cancellation tests use child-readiness handshakes and a logical
clock. Reader joins check that the reader has finished. A normal child exit
with a descendant still holding a pipe also gets bounded reader cleanup.
Tests control the cleanup grace and release descendants only after the probe
returns. They assert event order without elapsed-time limits. Process checks
assert that descendants stopped running; they do not assert zombie reaping.
