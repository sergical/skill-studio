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
target, size, or exact mtime invalidates a result. Failures and timeouts are
not cached. The default spawner has memory caching only; persistence is an
explicit opt-in to an app-owned path. The file is bounded and mode 0600.

Repeat with disposable directories:

```sh
cargo build -p skill-studio-host --example perf_detection --release --all-features
"$CARGO_TARGET_DIR/release/examples/perf_detection" /tmp/skill-probes-control uncached
"$CARGO_TARGET_DIR/release/examples/perf_detection" /tmp/skill-probes-cached
```

## Profile

`uncached.json.gz` and `cached.json.gz` are release Samply recordings with
symbol sidecars. `profile-metadata.json` records the source, binary hash, and
load. The folded stacks and SVGs retain stacks through `harness_detect` or
`probe_cache`. Observer sysctl/pgrep calls outside the timed interval are
excluded. Counts include sleeping and are not CPU utilization percentages.
The cache-enabled recording includes ten misses followed by ten hits.

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
The test's failure guard is not a performance budget or a wall-clock CI gate.
