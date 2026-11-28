# Undo, as a state chart

These charts show what a user can see while one change is undone: what
Activity shows, whether Undo is offered, and whether the files are back. The
arrows are what the user does and what the app does, including the app dying
between two steps. The `/pstack-statechart` skill drew them from
[model/UndoClaim.tla](model/UndoClaim.tla) and the words in
[model/view.mjs](model/view.mjs). [SKILL.md](SKILL.md) says how to draw them
again.

Colors: teal is at rest, blue is an undo that is running, yellow is an undo
that was cut off. If a rule breaks, the chart draws that state red and marks
the shortest path to it with a red border. Neither chart has one today.

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
  s4 --> s3: the app dies
  s3 --> s0: the next write clears the mark (files not back)
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
- If the app dies after the swap is sealed but before the undo is recorded as
  finished, the files stay back and the change is still shown as undone. The
  next write clears the mark. Undo is then offered again and asks for Force,
  because the files no longer match the change.
- If the app dies before the swap is sealed, the next write, or the desktop
  app starting again, puts the swapped-out folder back.

## A change the desktop wrote

Undo from Activity for a change the desktop's own event store wrote, such as
a link repair or a frontmatter repair made in the app. This chart is the
undo whose write lands at once. Every rule holds.

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
  s3: Undo running: shown as undone, files back
  s4: Shown as undone, but no undo finished
  s5: Undone. Files are back
  s6: Files are back, but Undo is still offered (it asks for Force)
  [*] --> s0
  s0 --> s1: you press Undo or you press Undo with Force
  s1 --> s0: the app dies or the undo ends without undoing
  s1 --> s2: the change is marked undone
  s2 --> s3: the files are put back
  s2 --> s4: the app dies
  s3 --> s4: the app dies
  s3 --> s5: the undo is recorded as finished
  s4 --> s1: you press Undo or you press Undo with Force
  s2 --> s1: a file write failed: the mark is cleared
  s5 --> s1: you press Undo or you press Undo with Force
  s4 --> s0: the next write clears the mark (files not back)
  s1 --> s4: Undo is refused: already undone
  s4 --> s6: the next write clears the mark (files back)
  s1 --> s5: the app dies or Undo is refused: already undone
  s6 --> s1: you press Undo or you press Undo with Force
  s1 --> s6: the undo ends without undoing
  s1 --> s3: the change is marked undone
  s3 --> s1: a file write failed: the mark is cleared
  class s0 teal
  class s1 blue
  class s2 blue
  class s3 blue
  class s4 yellow
  class s5 teal
  class s6 yellow
```

What to notice:

- The desktop records the undo before it marks the change. If the app dies
  between the two, nothing shows: the chart goes from "Undo started" back to
  "Change in place".
- An undo that was cut off after the mark leaves the change shown as undone
  until the next core write starts. That write clears the mark, and Undo is
  offered again. The desktop app starting again does not clear it.
- Until [issue 787](https://github.com/sergical/skill-studio/issues/787) was
  fixed, the desktop marked the change first. This chart then had one more
  state, drawn red: the change shown as undone for good.

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
