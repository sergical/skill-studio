// ============================================================================
// Skill Studio - skill-api tests
// Every frontend invoke must name a registered Tauri command. The commands are
// read off the source (`skill-api.ts`'s `callCommand` literals and every other
// `invoke` call) and checked against `src-tauri/src/lib.rs`'s registration
// list read straight off disk, and against the dev harness mock.
// ============================================================================

import { describe, expect, it } from "vitest";
import skillApiSource from "./skill-api.ts?raw";
import mockTauriSource from "../dev/harness/mock-tauri.ts?raw";
// `?raw` reads straight off disk, the same as the two source imports above - no `node:fs`, so
// this stays typed without a Node types dependency this frontend workspace doesn't otherwise need.
import libRsSource from "../../src-tauri/src/lib.rs?raw";

/** Every command `skill-api.ts` sends through `callCommand`, read off the source, and every
 * `callCommand(...)` call whose first argument is not a string literal (so cannot be checked). */
interface CallCommandUses {
  commands: string[];
  nonLiteral: string[];
}

function callCommandUses(source: string): CallCommandUses {
  const commands: string[] = [];
  const nonLiteral: string[] = [];
  for (const match of source.matchAll(/(?<!function )\bcallCommand(?:<[^>]*>)?\(\s*([^,)]*)/g)) {
    const literal = match[1].match(/^"([a-z_]+)"$/);
    if (literal) commands.push(literal[1]);
    else nonLiteral.push(match[0]);
  }
  return { commands, nonLiteral };
}

/** `skills::mod::command_name,` -> `command_name` for every line inside `generate_handler![...]`
 * in `src-tauri/src/lib.rs`, read straight off disk - the ops surface the frontend may call. One
 * Rust path per line; takes the segment after the last `::`. */
function registeredCommandNames(libRs: string): Set<string> {
  const start = libRs.indexOf("generate_handler![");
  if (start < 0) {
    throw new Error("registeredCommandNames: could not find generate_handler![...] in lib.rs");
  }
  // A `]\s*\)` close, not a literal indentation string - rustfmt reformatting the
  // block's indentation must not break this parse.
  const closeMatch = /\]\s*\)/.exec(libRs.slice(start));
  if (!closeMatch) {
    throw new Error("registeredCommandNames: could not find generate_handler![...] in lib.rs");
  }
  const end = start + closeMatch.index;
  const body = libRs.slice(start, end);
  const names = new Set<string>();
  for (const line of body.split("\n")) {
    const trimmed = line.trim();
    if (trimmed.length === 0 || trimmed.startsWith("//")) continue;
    const m = trimmed.match(/^([\w:]+),$/);
    if (!m) continue;
    const segments = m[1].split("::");
    names.add(segments[segments.length - 1]);
  }
  return names;
}

type InvokeCall = { kind: "literal"; command: string } | { kind: "dynamic"; argument: string };

const TAURI_CORE_IMPORT =
  /import\s+(?!type\b)(?:\{([^}]*)\}|\*\s+as\s+(\w+))\s+from\s+["']@tauri-apps\/api\/core["']/g;

/** The local names a file calls Tauri's `invoke` through: `invoke`, an `invoke as x` alias, or
 * `ns.invoke` for a namespace import. Type-only specifiers bind nothing callable. */
function invokeBindings(source: string): string[] {
  const names: string[] = [];
  for (const [, specifiers, namespace] of source.matchAll(TAURI_CORE_IMPORT)) {
    if (namespace) names.push(`${namespace}.invoke`);
    for (const specifier of specifiers?.split(",") ?? []) {
      const m = specifier.trim().match(/^(?!type\b)invoke(?:\s+as\s+(\w+))?$/);
      if (m) names.push(m[1] ?? "invoke");
    }
  }
  return names;
}

/** Index just past a balanced `<...>` type-argument list that starts at `start`. `=>` inside a
 * function type is not a closing bracket. */
function skipTypeArguments(source: string, start: number): number {
  let depth = 0;
  for (let i = start; i < source.length; i++) {
    const ch = source[i];
    if (ch === "<") depth++;
    else if (ch === ">" && source[i - 1] !== "=") {
      depth--;
      if (depth === 0) return i + 1;
    }
  }
  return source.length;
}

/** Every call through an `invoke` binding, with its first argument: a plain string literal
 * (any quote, a template only without `${}`) or the dynamic expression text. */
function invokeCalls(source: string): InvokeCall[] {
  const calls: InvokeCall[] = [];
  for (const name of invokeBindings(source)) {
    const escaped = name.replace(/[.$]/g, "\\$&");
    for (const match of source.matchAll(new RegExp(`(?<![\\w.$])${escaped}\\b`, "g"))) {
      let i = (match.index ?? 0) + match[0].length;
      while (/\s/.test(source[i] ?? "")) i++;
      if (source[i] === "<") i = skipTypeArguments(source, i);
      while (/\s/.test(source[i] ?? "")) i++;
      if (source[i] !== "(") continue;
      i++;
      while (/\s/.test(source[i] ?? "")) i++;
      const quote = source[i];
      if (quote === '"' || quote === "'" || quote === "`") {
        const close = source.indexOf(quote, i + 1);
        const text = source.slice(i + 1, close);
        if (quote !== "`" || !text.includes("${")) {
          calls.push({ kind: "literal", command: text });
          continue;
        }
      }
      calls.push({ kind: "dynamic", argument: source.slice(i, i + 40).split(/[,)]/)[0] });
    }
  }
  return calls;
}

/** `skill-api.ts`'s `callCommand` is the one place a command name may be a variable; the
 * wrapper-set test below pins every command that flows into it. */
const DYNAMIC_INVOKE_FILE = "./skill-api.ts";

describe("frontend invoke scanner", () => {
  it("invoke_scanner_reads_every_call_form_or_names_the_form_it_skips", () => {
    const fixture = [
      'import { invoke as call, type InvokeArgs } from "@tauri-apps/api/core";',
      'import * as core from "@tauri-apps/api/core";',
      "call(`backtick_command`);",
      'call<{ a: string; b: number }>("generic_command");',
      "call<(x: number) => void>('arrow_generic_command');",
      'core.invoke("namespace_command");',
      "call(commandName, args);",
      "call(`templated_${suffix}`);",
      'invoke("not_bound_here");',
    ].join("\n");
    expect(invokeCalls(fixture)).toEqual([
      { kind: "literal", command: "backtick_command" },
      { kind: "literal", command: "generic_command" },
      { kind: "literal", command: "arrow_generic_command" },
      { kind: "dynamic", argument: "commandName" },
      { kind: "dynamic", argument: "`templated_${suffix}`" },
      { kind: "literal", command: "namespace_command" },
    ]);
  });
});

describe("skill-api wrapper set", () => {
  it("every_frontend_invoke_names_a_registered_command", () => {
    const sources = import.meta.glob<string>("../**/*.{ts,tsx}", {
      query: "?raw",
      import: "default",
      eager: true,
    });
    const registered = registeredCommandNames(libRsSource);
    const missing: string[] = [];
    for (const [path, source] of Object.entries(sources)) {
      if (path.includes(".test.")) continue;
      for (const call of invokeCalls(source)) {
        if (call.kind === "literal" && !registered.has(call.command)) {
          missing.push(`${path}: ${call.command}`);
        } else if (call.kind === "dynamic" && path !== DYNAMIC_INVOKE_FILE) {
          missing.push(`${path}: dynamic command ${call.argument}`);
        }
      }
    }
    expect(missing, missing.join("\n")).toEqual([]);
  });

  it("every_skill_api_wrapper_command_is_registered_or_names_the_drift", () => {
    const { commands, nonLiteral } = callCommandUses(skillApiSource);
    expect(nonLiteral, "callCommand needs a literal command name to be checked").toEqual([]);
    expect(commands.length).toBeGreaterThan(0);
    const registered = registeredCommandNames(libRsSource);
    const missing = commands.filter((command) => !registered.has(command));
    expect(missing, `missing from generate_handler!: ${missing.join(", ")}`).toEqual([]);
  });

  it("dev_harness_mock_answers_every_command_the_kept_wrappers_call_or_names_the_gap", () => {
    const mockedCases = new Set(
      [...mockTauriSource.matchAll(/case "([a-z_]+)":/g)].map((m) => m[1]),
    );
    const missing = callCommandUses(skillApiSource).commands.filter(
      (command) => !mockedCases.has(command),
    );
    expect(missing, `dev harness mock-tauri.ts has no case for: ${missing.join(", ")}`).toEqual([]);
  });
});
