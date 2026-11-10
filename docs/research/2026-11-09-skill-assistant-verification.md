# Skill Assistant verification

Use the [verification guide](../../examples/skill-tests/README.md) to repeat these checks. The [design record](2026-11-09-skill-assistant.md) explains the alternatives and client limits.

## Native evidence

| Run                                    | Observed result                                                                                                                                                 |
| -------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `44bff486-bb18-469f-a65d-030104eac370` | Codex, Claude Code, OpenCode, and pi each completed the report task and passed its three artifact checks.                                                       |
| `d8278ee0-c09a-42b1-aadb-3d4110996a0e` | Pi candidate passed twice. No-skill control failed the skill-specific verification-line check twice; report existence and source facts passed in both variants. |
| `83d4fbe1-370f-4d12-9fb6-6acf94c96991` | A separate pi judge accepted factual accuracy. The deliberately absent file still failed the overall trial.                                                     |

Full local records remain in `/tmp/skill-studio-lab-verification`. Sanitized native streams are checked into `crates/skill-studio-eval/tests/fixtures`; the four-client result is also available in the read-only development preview. This is a mechanism check with one small task, not evidence of broad skill quality.

## Automated checks

- Full `pnpm run test`: 929 tests passed before the final two regression tests were added.
- Final scoped `vitest` run for `SkillLab.test.tsx` and `SkillPage.test.tsx`: 9 tests passed. This includes opening recorded results, closing a running dialog without cancellation, import rejection, and the native export request.
- `cargo test -p skill-studio-eval --lib`: 23 tests passed, including timeout/cancellation, native exit-zero errors, loading evidence, aliases, snapshots, recovery, history, and independent human review.
- `cargo test -p skill-studio --lib skills::skill_agent_runner::tests`: 42 existing runner tests passed after protocol extraction.
- TypeScript check, lint, React Doctor, unused-code check, formatting, and generated-type check passed. Final `cargo clippy -p skill-studio-eval -p skill-studio-cli -p skill-studio --all-targets -- -D warnings` also passed after the review fixes.

Browser checks covered setup, saved history, the comparison table, and the expanded report artifact at a desktop viewport. No console errors were reported. A selector failure in the first result-navigation attempt was corrected; that session was closed. The browser preview does not prove native process execution: the native evidence above comes from the shared CLI engine. The native save panel is covered by command compilation and the IPC flow test; it was not clicked through in a running desktop build.

## Post-green simplification

Applied: `clients.rs` no longer builds an unreachable Codex judge option. Unsupported judges still return the same error. The 21 then-current engine tests and 8 then-current scoped UI tests passed again, as did Clippy for the evaluation crate, CLI, and desktop.

Not applied: the shared protocol keeps its public surface and legacy-runner consumers. The store's public accessors were retained. Merging the old assistant process runner with suite execution would change its lifecycle and is outside a behavior-preserving cleanup.

Follow-ups: a container mode for CI and optional benchmark-framework adapters are separate features. The current implementation tests installed clients and records that environment's limits.

## Independent review

Two fresh-context reviews covered the engine and the desktop flow. Four findings were fixed: native export, folder/file skill identity in shared history, history truncation across skills, and macOS path aliases in loading evidence. Regression tests cover the changed contracts. Both reviewers found no remaining actionable issues in the revised source.
