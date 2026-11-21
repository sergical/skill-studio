# Missing locked skills

Check whether an install ledger lists a skill whose folder is absent.

## Sub-features

- `outdated` shows `currency: "missing_on_disk"`.
- `library --source <source>` shows `missing_on_disk: true`.

## How to get to it (user POV)

Run `outdated` to look for missing skills and `library --source <source>` to
narrow the report to a known upstream.

## Driving it with control-skill-studio

Add a skill from the test pack and run `track` to seed its lock row. Move its
folder out of the test home's skill roots, leaving the lock row in place.
Then run `cli -- outdated` and `cli -- library --source <source>`, using the
source recorded by `state`. Check that both reports name the skill and show
its missing status.

## Gotchas

Use `state` to confirm the lock row and the missing folder. A parked folder
still exists on disk, so parking a skill does not exercise this case.
