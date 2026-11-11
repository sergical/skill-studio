Usage fixtures are zstd-compressed Codex JSONL rollouts. Each has a
`session_meta` for `codex-one` at `/work` and one user skill block for `gamma`
at `2026-09-29T12:00:00Z`.

`usage-large.jsonl.zst` inserts 90 JSON records before the skill block. Each
record is `{"padding":"..."}` with 200,000 `x` characters. Its decoded size
exceeds the 16 MiB per-file budget. The usage flow test must resume decoding
and keep the session context to reach the skill block.

Generate with `zstd -q -f input.jsonl -o output.jsonl.zst`.
