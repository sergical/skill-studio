# Undo one change

Every write records a journal event with a backup and an inverse. A user can reverse one event; the change to other skills stays.

## Sub-features

- `undo-install` removes one skill installed in a batch, and only that one.
- `undo-update` puts back one skill's files from before an update.

## How to get to it (user POV)

- `skill-studio restore --event-id <id>`, `skill-studio undo` (the last change), `skill-studio events` to list ids.
- Desktop: Activity.

## Driving it with control-skill-studio

Preconditions: two skills added in one `pack add`.

- **Find the event.** Read `state.event_id` of the first step in the add's `.json`, or run `control-skill-studio cli -- events`.
- **Undo it.** Run `control-skill-studio cli -- restore --event-id <id>`. Exit 0.
- **Second view.** Run `control-skill-studio state`. That skill's folder is gone; the other skill's folder is still there, and the journal has a `restore` row.

## Gotchas

- `restore` takes `--event-id <id>`, not a positional id.
- An event whose files changed afterwards (for example, an install followed by an update) is refused without `--force`; undo the newest event first.
