# Skill testing

Skill Studio tests installed agent clients against saved requests and expected results.

## Language

**Suite**: A saved set of cases, variants, repetitions, and checking settings.
_Avoid_: Experiment, benchmark (unless discussing a published benchmark).

**Case**: One user request, its starting files, and its expected results.
_Avoid_: Prompt (which is only part of a case).

**Variant**: A client, optional model, and skill selection to compare on the same cases.
_Avoid_: Agent configuration.

**Trial**: One execution of a case and variant in one repetition.
_Avoid_: Run (which contains the full suite).

**Run**: A saved execution of a suite with frozen inputs and one or more trials.
_Avoid_: Session (which belongs to the native client).

**Loading evidence**: A successful native tool event that shows the target skill was read. Missing evidence means unknown.
_Avoid_: Trigger rate (unless a defined set of trigger cases was measured).

**Outcome**: The result of the checks, separate from whether the client completed.
_Avoid_: Success (which can hide this distinction).
