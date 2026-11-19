# Skill Studio verification map

The maintained source for verifying Skill Studio's user-facing behavior. Read
this index, then use the matching feature file as the recipe.

## Baseline preconditions

- A run from `control-skill-studio launch`, and `control-skill-studio doctor` passes.
- The run's home starts empty: no skills, no lock file, no journal.
- The pack at `pack-url` holds `bench-skill-00` to `bench-skill-NN`.
- Never drive the real home: every CLI call goes through `control-skill-studio cli`, which passes `--home`.

## Driving conventions

- CLI: `control-skill-studio cli -- <skill-studio args>`. Treat each command as literal.
- Desktop UI: Playwright on the dev harness, with ARIA roles and accessible names (see `desktop-harness.md`).
- Start each recipe from the baseline unless it says otherwise.

## Proof and skip reporting

- CLI proof: the `.cmd`, `.json`, `.stderr`, `.exit` and `.npx` files of each call, and a read-only second view (`state`, `scan`, `events`).
- UI proof: the spec name and its pass on WebKit and Chromium, or a Playwright trace or screenshot.
- Name the entry point used. Do not report a skipped entry point as verified through another one.
- A desktop-harness run is not proof of backend behavior; pair it with a CLI run of the same core op.

## Features

| File                                     | Feature                         | Entry points                                                          |
| ---------------------------------------- | ------------------------------- | --------------------------------------------------------------------- |
| [add-from-repo.md](add-from-repo.md)     | Add skills from a repo or pack  | `pack add`, `add`, Add Skill sheet, Packs → Add N new                 |
| [pack-update.md](pack-update.md)         | Update a pack or every pack     | `pack update`, `update`, Packs → Update / Update all, Home Update all |
| [undo.md](undo.md)                       | Undo one change                 | `restore --event-id`, `undo`, Activity                                |
| [park-unpark.md](park-unpark.md)         | Turn skills off and on          | `park`, `unpark`, selection bar Park / Unpark                         |
| [desktop-harness.md](desktop-harness.md) | Desktop UI flows on the harness | `pnpm --filter skill-studio run test:e2e`                             |

## Feature entry contract

Each feature file has an H1, one paragraph, then four H2s in this order:
`Sub-features`, `How to get to it (user POV)`, `Driving it with control-skill-studio`
(or `with Playwright`), and `Gotchas`.
