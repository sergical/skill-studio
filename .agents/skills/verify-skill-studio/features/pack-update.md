# Update a pack

A user updates every skill of a pack, or of every pack. The skills.sh skills of one scope update with one `npx skills update a b c` call per run of consecutive requests (#687), each with its own backup and event. Skills with local edits are skipped.

## Sub-features

- `update-batch` updates the skills of one scope with one `npx` call.
- `update-undo` gives each skill its own update event.
- `update-skip-edited` skips a skill whose folder no longer matches its lock hash.

## How to get to it (user POV)

- `skill-studio pack update <source>` or `pack update --all`.
- `skill-studio update --skill a --skill b`.
- Desktop: Packs → a row's Update, or Update all; Home → Update all.

## Driving it with control-skill-studio

Preconditions: baseline, then `pack add` of three pack skills, then `control-skill-studio track`.

- **Plan.** Run `control-skill-studio cli -- pack update --all --dry-run`. Three steps `planned`, 0 `npx` calls.
- **Update.** Run `control-skill-studio cli -- pack update --all`. Three steps `done` with different `event_id`s and 1 `npx` call: `skills@1.8.1 update <three names> --global`.
- **Second view.** Run `control-skill-studio state`. The journal has three `update` rows.

## Gotchas

- Without `track`, `pack update --all` plans no step: no pack is tracked for a loopback URL.
- The local pack does not change between add and update, so the update rewrites the same files. That proves the call count and the rows, not that new upstream content arrives.
