# Batch version cache writes

**Provisional: recorded under load.** Before measurements and profiles are in
`../detection/`. After measurements are in `after.json`, with raw samples,
load average, cargo/rustc counts, source SHA, binary hash, and hardware.
Both releases ran the same six-native-agent fixture. Each sample creates and
drops a runtime and spawner. Cleanup is inside the timed interval.

| Persistent cache phase | N each | Before median ms | After median ms | Before p95 ms | After p95 ms |
| ---------------------- | -----: | ---------------: | --------------: | ------------: | -----------: |
| Miss                   |     10 |           95.633 |          97.340 |        98.468 |       99.126 |
| Hit                    |     10 |            2.637 |           1.997 |         3.279 |        2.569 |

These loaded samples do not establish a latency improvement. Miss median
increased by 1.8%; hit median decreased by 24.3%. The hit path does no new
work that explains that change. A quiet rerun is needed for a latency claim.
The deterministic improvement is six cache file rewrites reduced to one for
a six-agent miss. Hits do not rewrite the cache.

The release Samply profile and symbols are committed beside this report.
The folded SVG retains stacks through `harness_detect` or `probe_cache` and
excludes observer processes. The profile includes ten misses and ten hits.
Sample counts include sleeps and are not CPU utilization percentages.

The cache flush now runs when the spawner finishes. Desktop detection owns
and drops its runtime inside the blocking closure. A process killed before
cleanup loses only reusable cache results; the next launch probes again.
No skill state depends on this file.

The new test checks that no file appears during a batch and that all results
reopen after cleanup. It failed on the eager-write implementation. Existing
cache invalidation and new-spawner reuse tests remain in the host test suite.
