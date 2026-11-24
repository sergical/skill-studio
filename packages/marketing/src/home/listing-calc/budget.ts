// Listing budget estimate for the homepage calculator. Equal-length example skills, no model
// names, a context window only.

export const CONTEXT_WINDOWS = [
  { tokens: 128_000, label: "128K" },
  { tokens: 200_000, label: "200K" },
  { tokens: 400_000, label: "400K" },
  { tokens: 1_000_000, label: "1M" },
] as const;

export const DEFAULT_WINDOW = 3;
export const DEFAULT_SKILLS = 215;
export const DEFAULT_PROJECT_SKILLS = 30;
export const MIN_SKILLS = 10;
export const MAX_SKILLS = 400;
export const MAX_PROJECT_SKILLS = 100;

const CHARS_PER_TOKEN = 4;
const NAME = 20;
const DESCRIPTION = 240;
// `- name: (file: r0/name/SKILL.md)` plus its newline, with Codex's short root alias.
const CODEX_LINE = 66;

/** `skillListingBudgetFraction`: Claude Code's default, and the stops the calculator offers. */
const CLAUDE_DEFAULT_FRACTION = 0.01;
// Official Claude Code settings: fraction > 0 and ≤ 1 (100%), default 0.01.
// https://code.claude.com/docs/en/settings-reference#skilllistingbudgetfraction
// Skill Studio mirrors that filter in listing_cost.rs. The calculator soft-caps at 10%:
// above that the listing usually costs more than it helps; this page teaches a budget, not a sink.
export const CLAUDE_FRACTIONS = [0.001, 0.0025, 0.005, 0.01, 0.02, 0.05, 0.1];
export const CLAUDE_DEFAULT_FRACTION_INDEX = CLAUDE_FRACTIONS.indexOf(CLAUDE_DEFAULT_FRACTION);
// Codex sizes its list at 2% of the window. Only an explicit `skills.max_context_tokens`
// replaces that, and Codex caps that setting, not the 2% default, at 10,000 tokens.
const CODEX_SHARE = 0.02;
export const CODEX_MAX_CONTEXT_TOKENS = 10_000;

export interface ClaudeEstimate {
  agent: "claude";
  total: number;
  fraction: number;
  /** The fraction of the window, in tokens. Claude Code counts the list in characters. */
  budgetTokens: number;
  budget: number;
  needed: number;
  described: number;
  nameOnly: number;
}

export interface CodexEstimate {
  agent: "codex";
  total: number;
  /** The `skills.max_context_tokens` in use, or null for 2% of the window. */
  override: number | null;
  budget: number;
  needed: number;
  full: number;
  shortened: number;
  leftOut: number;
  /** Characters each shortened description keeps. */
  kept: number;
}

/** Claude Code lists every name first, then adds descriptions until its budget runs out. */
export function estimateClaude(
  contextTokens: number,
  total: number,
  fraction = CLAUDE_DEFAULT_FRACTION,
): ClaudeEstimate {
  const budgetTokens = contextTokens * fraction;
  const budget = Math.max(1, Math.floor(budgetTokens * CHARS_PER_TOKEN));
  const base = { agent: "claude" as const, total, fraction, budgetTokens, budget };
  const nameWidth = NAME + 2;
  const fullWidth = NAME + 4 + DESCRIPTION;
  const needed = total * fullWidth;
  if (needed <= budget) return { ...base, needed, described: total, nameOnly: 0 };
  const left = budget - total * nameWidth;
  const described = Math.max(0, Math.min(total, Math.floor(left / (fullWidth - nameWidth))));
  return { ...base, needed, described, nameOnly: total - described };
}

function codexBudget(contextTokens: number, override: number | null) {
  if (override !== null) return Math.min(CODEX_MAX_CONTEXT_TOKENS, Math.max(1, override));
  return Math.max(1, Math.floor(contextTokens * CODEX_SHARE));
}

/** Codex trims every description by the same amount, then leaves whole skills out. */
export function estimateCodex(
  contextTokens: number,
  total: number,
  override: number | null = null,
): CodexEstimate {
  const budget = codexBudget(contextTokens, override);
  const room = budget * CHARS_PER_TOKEN;
  const needed = Math.ceil((total * (CODEX_LINE + DESCRIPTION + 1)) / CHARS_PER_TOKEN);
  const base = { agent: "codex" as const, total, override, budget, needed };
  if (needed <= budget)
    return { ...base, full: total, shortened: 0, leftOut: 0, kept: DESCRIPTION };
  const share = Math.floor((room - total * CODEX_LINE) / total) - 1;
  if (share >= 1) return { ...base, full: 0, shortened: total, leftOut: 0, kept: share };
  const listed = Math.min(total, Math.floor(room / CODEX_LINE));
  return { ...base, full: 0, shortened: 0, leftOut: total - listed, kept: 0 };
}

const NUMBER = new Intl.NumberFormat("en-US");
export const format = (value: number) => NUMBER.format(value);

export function claudeSentence(result: ClaudeEstimate) {
  if (result.nameOnly === 0) return `All ${format(result.total)} skills keep their description.`;
  if (result.described === 0) return `Every skill shows by name only.`;
  return `${format(result.nameOnly)} ${result.nameOnly === 1 ? "skill shows" : "skills show"} by name only.`;
}

export function claudeFollowUp(result: ClaudeEstimate) {
  if (result.nameOnly === 0) return "The model can pick any of them from what you ask.";
  if (result.described === 0) return "The model has to guess which to run from the name alone.";
  return `The model has to guess when to run ${result.nameOnly === 1 ? "it" : "them"} from the name alone.`;
}

export function codexSentence(result: CodexEstimate) {
  if (result.full === result.total) return `All ${format(result.total)} descriptions fit.`;
  if (result.shortened > 0)
    return `Every description is cut to about ${format(result.kept)} characters.`;
  return `${format(result.leftOut)} of ${format(result.total)} skills are left out of the list.`;
}

export function codexFollowUp(result: CodexEstimate) {
  if (result.full === result.total) return "Codex lists each skill in full.";
  if (result.shortened > 0)
    return `Codex trims all ${format(result.total)} by the same amount, from ${format(DESCRIPTION)}.`;
  return "Codex runs out of room for names and paths, so the rest are not listed at all.";
}

const PERCENT = new Intl.NumberFormat("en-US", { style: "percent", maximumFractionDigits: 2 });
export const formatPercent = (fraction: number) => PERCENT.format(fraction);

export function claudeBudgetLine(result: ClaudeEstimate, windowLabel: string) {
  return `${formatPercent(result.fraction)} of ${windowLabel} is ${format(Math.round(result.budgetTokens))} tokens, about ${format(result.budget)} characters.`;
}

export function codexBudgetLine(result: CodexEstimate, windowLabel: string) {
  if (result.override === null)
    return `${formatPercent(CODEX_SHARE)} of ${windowLabel} is ${format(result.budget)} tokens.`;
  return `${format(result.budget)} tokens at any context window. Codex caps this setting at ${format(CODEX_MAX_CONTEXT_TOKENS)}.`;
}

export const ESTIMATE_NOTE = `Estimated with a short name and a ${format(DESCRIPTION)}-character description per skill.`;
