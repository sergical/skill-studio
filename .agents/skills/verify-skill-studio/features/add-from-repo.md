# Add skills from a repo

A user installs one or more skills from a git source. Several skills of one source and scope cost one `npx skills add` call (#687), and each skill gets its own journal event, so each can be undone alone.

## Sub-features

- `add-batch` installs several skills of one source with one `npx` call.
- `add-undo` gives each skill its own install event.
- `add-skip-installed` skips a skill already installed in the scope (pack add plans it as skipped; the Add Skill sheet shows it as Installed).
- `add-dry-run` plans and changes nothing.

## How to get to it (user POV)

- `skill-studio pack add <source> --skill a --skill b`.
- `skill-studio add` for one skill.
- Desktop: the Add Skill sheet (⌘N, sidebar, or Packs → Add from a repo), and Packs → a row's Add N new.

## Driving it with control-skill-studio

Preconditions: baseline; `url` is the run's `pack-url`.

- **Dry run.** Run `control-skill-studio cli -- pack add "$url" --skill bench-skill-00 --skill bench-skill-01 --dry-run`. Exit 0, every step `planned`, 0 `npx` calls, and `state` lists no skill.
- **Add two.** Run `control-skill-studio cli -- pack add "$url" --skill bench-skill-00 --skill bench-skill-01`. Exit 0, both steps `done` with different `event_id`s, and exactly 1 `npx` call whose argv has both `--skill` flags.
- **Second view.** Run `control-skill-studio state`. Both folders exist under `home/.agents/skills`, and the journal has two `install` rows.
- **Skip installed.** Run `control-skill-studio cli -- pack add "$url" --skill bench-skill-00 --skill bench-skill-02`. `bench-skill-00` is `skipped` ("already installed here"), `bench-skill-02` is `done`, 1 `npx` call naming only `bench-skill-02`.

## Gotchas

- `npx skills@1.8.1` writes no lock row for a loopback git URL (127.0.0.1 or localhost). After an add, `pack list` shows no pack until you run `control-skill-studio track`. A GitHub source does get lock rows.
- Times depend on machine load (a `cargo` build or test run nearby doubles them). Compare times only across runs of the same shape on an idle machine; `tools/perf-writes.py` does that over several runs.
- `pack add` without `--skill` lists the source with git; that path needs the pack server too.
