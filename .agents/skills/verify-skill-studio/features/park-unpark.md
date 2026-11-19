# Park and unpark

Park turns a skill off for every agent by moving it out of the folders agents read; unpark puts it back. Neither runs `npx`.

## Sub-features

- `park-many` parks several skills in one call.
- `unpark-many` puts them back.

## How to get to it (user POV)

- `skill-studio park <names>...`, `skill-studio unpark <names>...` (aliases `disable`, `enable`).
- Desktop: the selection bar's Park and Unpark on Home and Skills.

## Driving it with control-skill-studio

Preconditions: two pack skills added.

- **Park.** Run `control-skill-studio cli -- park bench-skill-00 bench-skill-01`. Exit 0 and 0 `npx` calls.
- **Second view.** Run `control-skill-studio state`. Neither folder is under `home/.agents/skills`. In `control-skill-studio cli -- scan`, each of the two skills has one deployment whose `root.kind` is `{"kind": "parked"}`.
- **Unpark.** Run `control-skill-studio cli -- unpark bench-skill-00 bench-skill-01`. Both folders are back.

## Gotchas

- `pack update` refuses a parked skill, because the skills CLI would fetch it again and undo the park.
