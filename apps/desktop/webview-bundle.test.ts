// ============================================================================
// Skill Studio - Webview bundle facts tests
// The names `webview-bundle.json` lists are the only frame, error class, and
// component names the Rust scrub lets reach Sentry, so a word that is only
// text in the bundle must stay out of the list.
// ============================================================================

import { parseAst } from "vite";
import { describe, expect, it } from "vitest";

import { declaredCodeNames } from "./webview-bundle";

const BUNDLE = `
function renderRow(){ return "secret skill alice"; }
var ForkPullError = class extends Error { retry(){} };
const SkillRow = () => jsx("div", { title: "private_skill" });
export const api = { loadSkill: function(){}, label: "secret.skill" };
`;

describe("webview bundle facts", () => {
  it("lists the declared function, class, method, and component names, so V8 frame names match", () => {
    const names = declaredCodeNames(parseAst(BUNDLE));

    for (const name of [
      "renderRow",
      "ForkPullError",
      "ForkPullError.retry",
      "retry",
      "SkillRow",
      "loadSkill",
    ]) {
      expect(names).toContain(name);
    }
  });

  it("leaves out words that are only string text or object keys, so a skill name in a string cannot pass the scrub", () => {
    const names = declaredCodeNames(parseAst(BUNDLE));

    for (const word of [
      "secret",
      "skill",
      "alice",
      "private_skill",
      "secret.skill",
      "div",
      "label",
      "title",
    ]) {
      expect(names).not.toContain(word);
    }
  });
});
