# Doctor scope and retained entries

Doctor reports errors for broken links, corrupt registries or journals, and
pending history events. A skill link may resolve anywhere inside the home
or a selected project. A target outside every scope is a warning.

Retained trash, quarantine entries, and orphaned parked or fork entries warn
after 30 days. `RETAINED_ENTRY_MAX_AGE_DAYS` defines the age once. A recorded
creation time takes precedence over mtime. Later journal or history references
keep the earliest recorded creation time. Mtime applies only when no record
exists. An invalid recorded time gets a warning; touching an old entry does
not reset its age. The count cap remains 20 entries per skill. Active parked
skills and active fork bases are excluded at any age. Doctor never deletes.

These link and retention rules are lead defaults for the owner to confirm or
change. They do not authorize cleanup.

Core fixtures cover all four link cases, every agent root and the shared
root, corrupt files, pending plans, retained ages, and active references.
A host integration test checks real SQLite pending events without changing
status. CLI, MCP stdio, and desktop adapter tests cover the same report.
The CLI's human output labels each violation as warning or error.

## Release measurement

**Provisional: recorded under load.** The 378-skill FixtureBuilder home has
239 global skills, Claude Code and Codex plugin caches, and three broken
SKILL.md files. Ten fresh CLI processes per version ran in alternating
before/after order after warming executable trust and OS caches.

| Metric          | Before median | After median | Before p95/max | After p95/max |
| --------------- | ------------: | -----------: | -------------: | ------------: |
| CLI doctor, ms  |       164.516 |      172.241 |        195.369 |       240.139 |
| Peak RSS, bytes |      40968192 |     40968192 |       41500672 |      41598976 |

The median increased 4.7%. The loaded p95 increased 22.9%, so both versions
were profiled in release mode. The reference is current main's core/host
code, built from desktop-status commit `301f6c69`; the doctor change is
`b639223f`. The full SHAs, binary hashes, hardware, raw samples, load average,
and cargo/rustc counts are in `doctor-before-after.json`.

`doctor-profile/` contains ten-iteration Samply recordings, symbol sidecars,
folded stacks, top-five summaries, and SVGs. These are complete CLI main-thread
samples, including startup and waits. They are not CPU utilization percentages.
The added scope checks require reading skill links and retained state. The
loaded tail is not a new CI budget. A quiet rerun is still needed.

Top five before leaf frames, 1880 samples:

| Frame             | Samples |
| ----------------- | ------: |
| `lstat`           |     382 |
| `__open_nocancel` |     175 |
| `__open`          |     174 |
| `read`            |     165 |
| `ftruncate`       |     143 |

Top five after leaf frames, 1695 samples:

| Frame                               | Samples |
| ----------------------------------- | ------: |
| `lstat`                             |     396 |
| `__open_nocancel`                   |     199 |
| `__open`                            |     150 |
| `libsystem_malloc.dylib unresolved` |     124 |
| `__getdirentries64`                 |     109 |

The shared-lease path accounts for 143 `ftruncate` samples in the before
profile and 5 in the after profile. Its code is unchanged between these
builds. This variation is a reason to keep the tail provisional. The lease
owner can check it in the quiet rerun; this change does not alter that path.
