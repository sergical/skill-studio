# Backup retention disk use

Measured on an Apple M3 Max, macOS APFS, with the real host history adapter.
The fixture makes 1,000 updates of one skill target. Each version has one
64 KiB file. Every version goes through `backup_paths`, `record` and successful
`finish`. Counts come from the core constants. All 1,000 history rows remain.

| Fixture age                               | Backup count | Logical file bytes | Allocated file bytes |
| ----------------------------------------- | -----------: | -----------------: | -------------------: |
| Before retention                          |        1,000 |         65,776,000 |           69,632,000 |
| After retention, all younger than 30 days |        1,000 |         65,776,000 |           69,632,000 |
| After retention, all older than 30 days   |           20 |          1,315,520 |            1,392,640 |

The age and count rules are a union. A burst of 1,000 updates in one day saves
no space immediately. Once those backups pass the age limit, this fixture keeps
the newest 20 and frees 98% of backup file space. Allocated bytes use
`st_blocks * 512`; directory and SQLite space are excluded on both sides.

Reproduce with the shared target folder:

```sh
cargo test -p skill-studio-host --all-features --lib thousand_updates_report -- --ignored --nocapture
```

The ignored test creates and removes its own temporary home. It contains no
wall-clock assertion. This is a disk-use measurement, not a timing benchmark.
