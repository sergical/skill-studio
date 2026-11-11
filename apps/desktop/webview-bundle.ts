// ============================================================================
// Skill Studio - Webview bundle facts
// Writes `webview-bundle.json`: the Sentry SDK version and the function,
// class, and method names the built bundle declares. The Rust webview scrub
// stamps that version and keeps a frame function or React component name
// only when it is exactly one of these names, so a skill, user, or path name
// cannot reach Sentry in those fields.
// ============================================================================

import { Visitor } from "vite";
import sentryReact from "@sentry/react/package.json" with { type: "json" };
import type { ESTree, Plugin } from "vite";

const BUNDLE_FILE = "webview-bundle.json";

const MAX_NAME_LENGTH = 64;

type Program = Parameters<Visitor["visit"]>[0];

function isFunctionOrClass(
  node: Pick<ESTree.Node, "type"> | null | undefined,
): node is ESTree.Function | ESTree.ArrowFunctionExpression | ESTree.Class {
  return (
    node?.type === "FunctionExpression" ||
    node?.type === "ArrowFunctionExpression" ||
    node?.type === "ClassExpression"
  );
}

/** The names V8 can report for functions and classes declared in `program`. */
export function declaredCodeNames(program: Program): Set<string> {
  const names = new Set<string>();
  const add = (name: string) => {
    if (name.length <= MAX_NAME_LENGTH) names.add(name);
  };
  const addClass = (name: string, node: ESTree.Class) => {
    add(name);
    for (const member of node.body.body) {
      if (member.type !== "MethodDefinition" || member.computed) continue;
      if (member.key.type !== "Identifier") continue;
      add(member.key.name);
      add(`${name}.${member.key.name}`);
    }
  };
  const addBinding = (name: string, value: ESTree.Expression | null | undefined) => {
    if (!isFunctionOrClass(value)) return;
    if (value.type === "ClassExpression") addClass(name, value);
    else add(name);
  };
  const addFunction = (node: ESTree.Function) => {
    if (node.id) add(node.id.name);
  };
  const addNamedClass = (node: ESTree.Class) => {
    if (node.id) addClass(node.id.name, node);
  };
  new Visitor({
    FunctionDeclaration: addFunction,
    FunctionExpression: addFunction,
    ClassDeclaration: addNamedClass,
    ClassExpression: addNamedClass,
    VariableDeclarator(node) {
      if (node.id.type === "Identifier") addBinding(node.id.name, node.init);
    },
    AssignmentExpression(node) {
      if (node.left.type === "Identifier") addBinding(node.left.name, node.right);
    },
    Property(node) {
      if (node.type !== "Property" || node.computed || node.key.type !== "Identifier") return;
      if (isFunctionOrClass(node.value)) add(node.key.name);
    },
  }).visit(program);
  return names;
}

export function webviewBundle(): Plugin {
  return {
    name: "skill-studio:webview-bundle",
    apply: "build",
    generateBundle(_options, bundle) {
      const names = new Set<string>();
      for (const output of Object.values(bundle)) {
        if (output.type !== "chunk") continue;
        for (const name of declaredCodeNames(this.parse(output.code))) names.add(name);
      }
      this.emitFile({
        type: "asset",
        fileName: BUNDLE_FILE,
        source: JSON.stringify({ sdkVersion: sentryReact.version, codeNames: [...names].sort() }),
      });
    },
  };
}
