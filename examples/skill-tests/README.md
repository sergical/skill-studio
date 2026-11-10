# Verify the Skill Assistant

These small suites test the whole path: native client, skill loading, saved files, checks, and retained evidence. They use your installed clients and existing sign-in. A real run consumes the selected client's usage. `clients` and `validate` do not make model requests.

From the repository root, use the standalone binary to avoid confusing it with an older installed Skill Studio CLI:

```sh
cargo run -p skill-studio-eval --bin skill-studio-eval -- clients
cargo run -p skill-studio-eval --bin skill-studio-eval -- validate examples/skill-tests/clients.json
cargo run -p skill-studio-eval --bin skill-studio-eval -- run examples/skill-tests/clients.json
```

In a worktree, set `CARGO_TARGET_DIR` to the main checkout's `target` first, as described in `AGENTS.md`. The main CLI provides the same commands under `skill-studio test`.

## Four useful checks

| Suite                  | Expected evidence                                                                                                                                       | Expected exit                           |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------- |
| `clients.json`         | Claude Code, Codex, OpenCode, and pi each create `report.md`, preserve the facts, and add the skill's verification line.                                | 0 if all four pass                      |
| `comparison.json`      | Two pi trials with the skill and two with discovery disabled. Inspect each check: the marker tests the specified workflow, not general writing quality. | 1 when a control fails the marker check |
| `planted-failure.json` | The client can finish successfully, but the deliberately missing `not-created.md` fails its file check.                                                 | 1                                       |

Run another suite by replacing the last path in the command. `judged-report.json` adds a separate pi judge for factual accuracy and keeps the deliberately missing-file check. The overall result must remain failed (exit 1), even when the judge accepts the accuracy criterion. If a client is missing or not signed in, its trial is not evaluated and cannot pass. Exit 1 means the saved suite did not pass in full, including incomplete trials; exit 2 means command setup or execution returned an error before a normal saved result was available.

Press Control-C to stop a CLI run. It stops the current process group and records cancellation. `history` lists saved runs; `show RUN_ID` prints the evidence. Add `--data-dir PATH` to keep demonstration runs separate. Default storage is the desktop application's `skill-lab` data folder.

## Installed skills

The [installed-skill cases](installed/README.md) cover code cleanup, forms, and reduced motion with six tasks. Their [recorded results](../../docs/research/2026-11-10-installed-skill-tests.md) distinguish those CLI checks from the real writing task exercised through the desktop.

## Desktop feature flag

All desktop assistant entry points use the existing `skill-assistant` flag. It defaults to off. This includes the single **Assistant** entry, its dedicated workspace, saved runs and evidence, skill improvement tools, and the recorded browser preview. When off, the workspace does not mount or ask for clients or saved suites.

For a local development trial, start Vite with `VITE_SKILL_ASSISTANT=1`. Production builds ignore that environment opt-in. The existing per-machine override, `localStorage.setItem("feature:skill-assistant", "on")`, still works after a reload; `"off"` takes precedence over the development opt-in. Remove that key to restore the default.

## Desktop flow

1. Open an installed skill and select **Assistant**. It opens a dedicated page.
2. Enter a **Task** and optional **Input files**. Select **Run task**. The default uses Codex once and asks you to review the output.
3. Open **Advanced options** only when needed: saved suites, automatic checks, clients/models, prior skill versions, repeated runs, and a separate quality judge. Pi supports a **No skill** comparison.
4. The **Result** view separates execution, skill loading, checks, output files, and your review. Created files come before unchanged inputs. Technical evidence and comparisons stay collapsed.
5. Add a review with a reason. Reviews are saved alongside the machine result. Export the result or reopen it from **Saved runs**.
6. Select the skill name in the breadcrumb to return. Reopening Assistant restores the task and review drafts while the skill page stays mounted. Save a suite before leaving the skill page or quitting. A running client continues after navigation.

Invalid JSON is retained across cases and views and blocks Save, Run, and Export until corrected. **Edit and run again** restores the saved run's exact input files. **Improve skill** contains the existing question and instruction-review tools; project testing is under its advanced tools.

Expected results include text, JSON values, file existence/content, tool use, loading evidence, and an acceptance command. Semantic criteria can use a separate Claude Code or pi judge with tools disabled. Missing or invalid judge output needs review and cannot erase a deterministic failure.

## Recorded preview

The development-only page `/?skill-lab=1` displays the real four-client run recorded on November 9, 2026. It is a read-only UI preview: it does not launch clients or save changes.

Install dependencies with `pnpm install --frozen-lockfile` if this checkout has no `node_modules` folder. Then start the preview:

```sh
VITE_SKILL_ASSISTANT=1 portless skill-assistant /bin/sh -c 'pnpm --filter skill-studio exec vite --port "$PORT" --host "$HOST"'
```

Open `https://skill-assistant.localhost/?skill-lab=1`, select **Saved runs**, then open **Report writer across installed clients**. Run this in a visible managed terminal. Use the desktop application or CLI for actual execution.

## Limits that affect interpretation

- Fresh folders and sessions do not provide an operating-system sandbox. Native clients retain their installed tools and sign-in; acceptance commands are explicit executable suite content.
- Claude Code, Codex, and OpenCode can retain ambient client configuration or skills. Their results describe that installed environment. Only pi currently exposes the supported no-skill control.
- A successful native skill read is positive evidence. No read event means unknown, not proof of absence.
- Costs and tokens appear only when the client reports them. Provider estimates are labeled. Different clients and models are not a controlled measure of skill quality.
- Rerunning takes a new source snapshot. Original inputs, hashes, versions, commands, raw streams, and result evidence remain in the saved run directory.
- Cursor and Grok Build are not supported by this test engine yet. The inspected Cursor version lacked the required controls; Grok was not installed for verification.

The example establishes that the evaluation path detects a concrete behavior and a planted failure. It does not establish broad skill quality from four tasks.
