# History list index

Measured on 2026-11-10, on an Apple M3 Max. Each case has two warm-up calls and
20 measured calls. Times include a new CLI process and JSON output to `/dev/null`.
The Rust binary uses the release profile. Other agents were building on this
machine. The p95, especially for the first case, includes that local contention.
No timing limit is added to CI.

Each temporary home has 100 skill names, no installed skill files, and either
10,000 or 100,000 completed install history rows. Five consecutive rows share a
timestamp. The page has 100 rows. Both schema states keep the existing
`idx_events_skill` index. The only variable is `idx_events_ts ON events(ts)`.
The same release binary runs both schema states to isolate the index's effect.
Both states return the expected IDs in timestamp and insertion order.

| Rows    | Schema               | Median, ms | p95, ms |
| ------- | -------------------- | ---------: | ------: |
| 10,000  | Before               |     30.753 | 149.085 |
| 10,000  | With timestamp index |     14.302 |  16.288 |
| 100,000 | Before               |     68.540 |  76.902 |
| 100,000 | With timestamp index |     13.476 |  14.634 |

`EXPLAIN QUERY PLAN SELECT * FROM events ORDER BY ts DESC, rowid DESC LIMIT 100`
returns this before the index at both sizes:

```text
SCAN events
USE TEMP B-TREE FOR ORDER BY
```

With the index, at both sizes:

```text
SCAN events USING INDEX idx_events_ts
```

An ascending timestamp index carries SQLite's ascending rowid tie order. The
reverse scan satisfies both descending keys. A descending timestamp index alone
would still need to sort equal-timestamp groups by rowid.

Because the time changed by more than 10%, samply 0.13.1 recorded 30 CLI calls
per schema state on the 100,000-row fixture, in save-only mode. The unindexed
profile has 1,456 sampled stacks; 1,376 include `sqlite3_step` and 473 include
`sqlite3BtreeInsert`. The indexed profile has 162 sampled stacks. Its remaining
work is mainly process startup, opening the database, reading its schema, and
closing it. The full-table scan and temporary index work no longer dominate.
Native frames were matched to the release binary's sorted symbol addresses.

The baseline release binary SHA-256 is
`38f840a1e441be046767a35f3b318ecc67f0c66962229d33d4fe14332397e71a`.
The full samples are in `history-list-samples.json` beside this report. The two opener regression tests
also check the real bundled SQLite query plan and timestamp ties. They fail
before this change with `SCAN events` and `USE TEMP B-TREE FOR ORDER BY`.
