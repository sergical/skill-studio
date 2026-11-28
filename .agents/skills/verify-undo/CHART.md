# Undo, as a state chart

These charts show what a user can see while one change is undone: what
Activity shows, whether Undo is offered, and whether the files are back. The
arrows are what the user does and what the app does, including the app dying
between two steps. The `/pstack-statechart` skill drew them from
[model/UndoClaim.tla](model/UndoClaim.tla) and the words in
[model/view.mjs](model/view.mjs). [SKILL.md](SKILL.md) says how to draw them
again.

Colors: teal is at rest, blue is an undo that is running, yellow is an undo
that was cut off. A red state breaks a rule, and a red border marks the
shortest path to it.

## A change the core wrote

Undo from Activity, `skill-studio restore` or the MCP tool `restore_event`,
for a change such as an update, a remove or an invocation change. This chart
is the undo that swaps a whole folder back, as for an update, with the swap
landing at once. Every rule holds.

```mermaid
---
title: "Undo of one change, when the app can die between two steps"
---
stateDiagram-v2
  state "Legend" as legend {
    lg0: at rest
    lg1: undo running
    lg2: cut off
  }
  class lg0 teal
  class lg1 blue
  class lg2 yellow
  s0: Change in place. Undo offered
  s1: Undo started. Nothing of it shows yet
  s2: Undo running: shown as undone, files not back
  s3: Shown as undone, but no undo finished
  s4: Undo running: shown as undone, files back
  s5: Undone. Files are back
  s6: Files are back, but Undo is still offered (it asks for Force)
  [*] --> s0
  s0 --> s1: you press Undo or you press Undo with Force
  s1 --> s0: the app dies or the undo is recorded as failed
  s1 --> s2: the change is marked undone
  s2 --> s3: the app dies
  s3 --> s1: you press Undo or you press Undo with Force
  s2 --> s1: a file write failed: the mark is cleared
  s2 --> s4: the files are put back
  s3 --> s0: the next write clears the mark (files not back)
  s4 --> s3: the app dies
  s4 --> s1: a file write failed: the mark is cleared
  s4 --> s5: the undo is recorded as finished
  s3 --> s6: the next write clears the mark (files back)
  s5 --> s1: you press Undo or you press Undo with Force
  s1 --> s6: the app dies or the undo is recorded as failed or Undo is refused: files changed since, it asks for Force
  s6 --> s1: you press Undo or you press Undo with Force
  s1 --> s5: the app dies or Undo is refused: already undone
  s6 --> s0: the next write reverses the unsealed folder swap
  s1 --> s4: the change is marked undone
  class s0 teal
  class s1 blue
  class s2 blue
  class s3 yellow
  class s4 blue
  class s5 teal
  class s6 yellow
  classDef teal fill:#bfe3df,stroke:#5a9b94,color:#1c1c1c
  classDef blue fill:#c3e0ff,stroke:#5b93cf,color:#1c1c1c
  classDef yellow fill:#f0dcbf,stroke:#b98f4d,color:#1c1c1c
  classDef bugPath stroke:#e72e3b,stroke-width:2.5px
```

What to notice:

- An undo that was cut off leaves the change shown as undone until the next
  write starts. That write clears the mark, and Undo is offered again.
- If the app dies after the files are back but before the undo is recorded as
  finished, the files stay back and Undo is still offered. Pressing it asks
  for Force, because the files no longer match the change.
- A folder swap that was not sealed is reversed by the next write, or when
  the desktop app starts again.

## A change the desktop wrote

Undo from Activity for a change the desktop's own event store wrote, such as
a link repair or a frontmatter repair made in the app. This chart is the
undo whose write lands at once. The rule `NoStrandedUndo` breaks: the red
state.

```mermaid
---
title: "Undo of one change, when the app can die between two steps"
---
stateDiagram-v2
  state "Legend" as legend {
    lg0: at rest
    lg1: undo running
    lg2: cut off
    lg3: Breaks NoStrandedUndo
    lg4: Path to the NoStrandedUndo bug
  }
  class lg0 teal
  class lg1 blue
  class lg2 yellow
  class lg3 breaks
  class lg4 bugPath
  s0: Change in place. Undo offered
  s1: Undo started. Nothing of it shows yet
  s2: Undo running: shown as undone, files not back
  s3: Shown as undone, but no undo finished
  s4: Undo running: shown as undone, files back
  s5: Undone. Files are back
  s6: Stuck: shown as undone for good. No undo finished, and Undo is refused
  s7: Files are back, but Undo is still offered (it asks for Force)
  [*] --> s0
  s0 --> s1: you press Undo or you press Undo with Force
  s1 --> s0: the app dies
  s1 --> s1: the checks pass
  s1 --> s2: the change is marked undone
  s2 --> s3: the app dies
  s3 --> s3: another write starts or the next write looks for a cut-off undo or the next write goes on
  s3 --> s1: you press Undo or you press Undo with Force
  s2 --> s4: the files are put back
  s1 --> s3: Undo is refused: already undone
  s4 --> s3: the app dies
  s4 --> s5: the undo is recorded as finished
  s3 --> s0: the next write clears the mark (files not back)
  s2 --> s0: a file write failed: the mark is cleared
  s5 --> s1: you press Undo or you press Undo with Force
  s3 --> s6: the next write finds no undo record, so the mark stays
  s3 --> s7: the next write clears the mark (files back)
  s1 --> s5: the app dies or Undo is refused: already undone
  s6 --> s1: you press Undo or you press Undo with Force
  s1 --> s6: Undo is refused: already undone
  s7 --> s1: you press Undo or you press Undo with Force
  s1 --> s4: the change is marked undone
  s4 --> s7: Undo is refused: files changed since, it asks for Force or a file write failed: the mark is cleared
  classDef breaks fill:#ffecee,stroke:#e72e3b,stroke-width:2.5px,color:#cd2834
  class s0 teal
  class s0 bugPath
  class s1 blue
  class s1 bugPath
  class s2 blue
  class s2 bugPath
  class s3 yellow
  class s3 bugPath
  class s4 blue
  class s5 teal
  class s6 yellow
  class s6 breaks
  class s6 bugPath
  class s7 yellow
  classDef teal fill:#bfe3df,stroke:#5a9b94,color:#1c1c1c
  classDef blue fill:#c3e0ff,stroke:#5b93cf,color:#1c1c1c
  classDef yellow fill:#f0dcbf,stroke:#b98f4d,color:#1c1c1c
  classDef bugPath stroke:#e72e3b,stroke-width:2.5px
```

The shortest path to the red state:

1. You press Undo.
2. The desktop marks the change as undone. It has not recorded the undo yet.
3. The app dies.
4. The next write looks for a cut-off undo to clear. It finds no undo record,
   so the mark stays.
5. The change is shown as undone for good. No undo finished, the files are
   not back, and Undo is refused.

## What the charts leave out

- Steps that do not change what the user can see, such as a check that
  passes.
- A recovery pass that starts when nothing is left to recover. The core chart
  also leaves out the desktop app starting again: it marks a cut-off undo and
  reverses an unsealed folder swap, as the next write does, but it does not
  clear the mark.
- A write that is caught half way. Most undo writes are not one atomic
  operation: the core removes an installed folder entry by entry, and the
  desktop removes the live path before it puts the old one in. If the app
  dies in between, the files are half put back and the change is still shown
  as undone. After the next write clears the mark, Undo is offered again and
  asks for Force. The check covers this; the charts would have about 30
  arrows with it.
- The charts use 2 Undo requests and 1 crash. The check uses 3 and 2.
