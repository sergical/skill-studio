export const MAX_DEMO_SKILLS = 600;
export const CLAUDE_DEFAULT_PERCENT = 1;
export const CLAUDE_DESCRIPTION_CAP = 1_536;
export const CLAUDE_EXPERIMENT_CAP = 100_000;
export const CODEX_DESCRIPTION_CAP = 1_024;
export const CODEX_OVERRIDE_CAP = 10_000;

export const CLAUDE_MODELS = [
  { label: "Opus 5.5", tokens: 1_000_000, charsPerToken: 3 },
  { label: "Sonnet 4.6", tokens: 1_000_000, charsPerToken: 4 },
  { label: "Haiku 4.5", tokens: 200_000, charsPerToken: 4 },
] as const;

export const CODEX_MODELS = [
  { label: "gpt-6.1-sol", tokens: 500_000 },
  { label: "gpt-6-astra", tokens: 272_000 },
  { label: "Unknown context", tokens: null },
] as const;

export type ListingState = "full" | "shortened" | "nameOnly" | "dropped";
export type BudgetUnit = "characters" | "tokens";

interface Budget {
  limit: number;
  unit: BudgetUnit;
}

interface ClaudeLine {
  nameWidth: number;
  fullWidth: number;
  score: number;
}

interface CodexLine {
  name: string;
  locator: string;
  description: string;
}

interface CodexCostLine {
  length: number;
  cost: (characters: number) => number;
}

interface CodexAllocation {
  needed: number;
  kept: Array<number | null>;
  used: number;
}

interface ListingEstimate extends Budget {
  needed: number;
  states: ListingState[];
  counts: Record<ListingState, number>;
}

interface ListingScenario {
  agent: "claude" | "codex";
  skillCount: number;
  description: string;
  claudeModel: (typeof CLAUDE_MODELS)[number];
  claudePercent: number;
  codexWindow: number | null;
  codexOverride: number | null;
}

const SAMPLE_NAMES = Array.from(
  { length: MAX_DEMO_SKILLS },
  (_, index) => `skill-${String(index + 1).padStart(3, "0")}`,
);
const EXAMPLE_ROOT = "/Users/you/.agents/skills";
const CODEX_ABSOLUTE_INTRO =
  "A skill is a set of instructions provided through a `SKILL.md` source. Below is the list of skills that can be used. Each entry includes a name, description, and source locator. `file` locators are on the host filesystem, `executor package` locators are owned by their execution environment, `cloud package` locators are opaque package identifiers, and `custom resource` locators use their provider's access mechanism.";
const CODEX_ALIAS_INTRO =
  "A skill is a set of local instructions to follow that is stored in a `SKILL.md` file. Below is the list of skills that can be used. Each entry includes a name, description, and a short path that can be expanded into an absolute path using the skill roots table.";

const encoder = new TextEncoder();

/** Mirrors claude_budget for a share chosen in the calculator. */
export function claudeBudgetChars(tokens: number, charsPerToken: number, percent: number) {
  const hundredths = Math.round(percent * 100);
  return Math.max(1, Math.floor((tokens * charsPerToken * hundredths) / 10_000));
}

/** Mirrors codex_budget. The cap applies to the user override. */
export function codexListingBudget(tokens: number | null, override: number | null): Budget {
  if (override !== null) {
    return { limit: Math.min(CODEX_OVERRIDE_CAP, Math.max(1, override)), unit: "tokens" };
  }
  if (tokens === null) return { limit: 8_000, unit: "characters" };
  return { limit: Math.max(1, Math.floor((tokens * 2) / 100)), unit: "tokens" };
}

/** Mirrors claude_allocate, including its newline cost and stable usage-order pass. */
export function allocateClaudeDescriptions(lines: readonly ClaudeLine[], budget: number) {
  const joins = Math.max(0, lines.length - 1);
  const full = lines.reduce((sum, line) => sum + line.fullWidth, joins);
  if (full <= budget) return lines.map(() => true);
  let left = budget - lines.reduce((sum, line) => sum + line.nameWidth, joins);
  const kept = lines.map(() => false);
  const order = lines.map((_, index) => index).sort((a, b) => lines[b].score - lines[a].score);
  for (const index of order) {
    const line = lines[index];
    const gain = line.fullWidth - line.nameWidth;
    if (gain <= left) {
      kept[index] = true;
      left -= gain;
    }
  }
  return kept;
}

function textSize(text: string, unit: BudgetUnit) {
  return unit === "tokens" ? encoder.encode(text).length : Array.from(text).length;
}

function quantize(size: number, unit: BudgetUnit) {
  return unit === "tokens" ? Math.ceil(size / 4) : size;
}

function prepareCodexLines(lines: readonly CodexLine[], unit: BudgetUnit) {
  const widths = new Map<string, number[]>();
  return lines.map(({ name, locator, description }): CodexCostLine => {
    let prefix = widths.get(description);
    if (!prefix) {
      prefix = [0];
      for (const character of description)
        prefix.push(prefix[prefix.length - 1] + textSize(character, unit));
      widths.set(description, prefix);
    }
    const base = textSize(`- ${name}: (file: ${locator})\n`, unit);
    return {
      length: prefix.length - 1,
      cost: (characters) => quantize(base + (characters === 0 ? 0 : prefix[characters] + 1), unit),
    };
  });
}

function allocateCodexLines(lines: readonly CodexCostLine[], budget: number): CodexAllocation {
  const costAt = (round: number) =>
    lines.reduce((sum, line) => sum + line.cost(Math.min(line.length, round)), 0);
  const maxLength = Math.max(0, ...lines.map((line) => line.length));
  const needed = costAt(maxLength);
  if (needed <= budget) return { needed, kept: lines.map((line) => line.length), used: needed };

  const minimum = costAt(0);
  if (minimum > budget) {
    let used = 0;
    const kept = lines.map((line) => {
      const next = used + line.cost(0);
      if (next > budget) return null;
      used = next;
      return 0;
    });
    return { needed, kept, used };
  }

  // Skip the complete round-robin passes, then spend the remainder in listing order.
  let low = 0;
  let high = maxLength;
  while (low + 1 < high) {
    const middle = Math.floor((low + high) / 2);
    if (costAt(middle) <= budget) low = middle;
    else high = middle;
  }
  const kept = lines.map((line) => Math.min(line.length, low));
  let used = costAt(low);
  let changed = true;
  while (changed) {
    changed = false;
    for (const [index, line] of lines.entries()) {
      const current = kept[index];
      if (current === line.length) continue;
      const next = used + line.cost(current + 1) - line.cost(current);
      if (next > budget) continue;
      kept[index] += 1;
      used = next;
      changed = true;
    }
  }
  return { needed, kept, used };
}

/** Mirrors codex_allocate for already capped descriptions in their listing order. */
export function allocateCodexDescriptions(lines: readonly CodexLine[], budget: Budget) {
  return allocateCodexLines(prepareCodexLines(lines, budget.unit), budget.limit);
}

function codexExampleAllocation(names: readonly string[], description: string, budget: Budget) {
  const lines = names.map((name) => ({
    name,
    description,
    locator: `${EXAMPLE_ROOT}/${name}/SKILL.md`,
  }));
  const absolute = allocateCodexDescriptions(lines, budget);
  const rootTable = `\n### Skill roots\n- \`r0\` = \`${EXAMPLE_ROOT}\``;
  const bodyCost = (intro: string, roots: string) =>
    quantize(
      textSize(`\n## Skills\n${intro}${roots}\n### Available skills\n`, budget.unit),
      budget.unit,
    );
  const overhead = Math.max(
    0,
    bodyCost(CODEX_ALIAS_INTRO, rootTable) - bodyCost(CODEX_ABSOLUTE_INTRO, ""),
  );
  if (overhead >= budget.limit) return absolute;

  const shortPaths = lines.map((line) => ({ ...line, locator: `r0/${line.name}/SKILL.md` }));
  const aliased = allocateCodexDescriptions(shortPaths, {
    ...budget,
    limit: budget.limit - overhead,
  });
  const quality = (allocation: CodexAllocation) => {
    const included = allocation.kept.filter((length) => length !== null);
    return (
      included.length * (names.length * CODEX_DESCRIPTION_CAP + 1) +
      included.reduce((sum, length) => sum + length, 0)
    );
  };
  const difference = quality(aliased) - quality(absolute);
  return difference > 0 || (difference === 0 && aliased.used + overhead < absolute.used)
    ? { ...aliased, needed: aliased.needed + overhead, used: aliased.used + overhead }
    : absolute;
}

/** Estimates the list of a single agent with equal-length example descriptions. */
export function estimateExampleListing(input: ListingScenario): ListingEstimate {
  const names = SAMPLE_NAMES.slice(0, input.skillCount);
  const description = Array.from(input.description);
  let budget: Budget;
  let needed: number;
  let states: ListingState[];
  if (input.agent === "claude") {
    budget = {
      limit: claudeBudgetChars(
        input.claudeModel.tokens,
        input.claudeModel.charsPerToken,
        input.claudePercent,
      ),
      unit: "characters",
    };
    const clipped = description.length > CLAUDE_DESCRIPTION_CAP;
    const width = clipped
      ? description.slice(0, CLAUDE_DESCRIPTION_CAP - 1).join("").length + 1
      : input.description.length;
    const lines = names.map((name, index) => ({
      nameWidth: name.length + 2,
      fullWidth: name.length + 4 + width,
      score: names.length - index,
    }));
    needed = lines.reduce((sum, line) => sum + line.fullWidth, Math.max(0, names.length - 1));
    states = allocateClaudeDescriptions(lines, budget.limit).map((kept) =>
      kept ? (clipped ? "shortened" : "full") : "nameOnly",
    );
  } else {
    budget = codexListingBudget(input.codexWindow, input.codexOverride);
    const clipped = description.length > CODEX_DESCRIPTION_CAP;
    const capped = clipped
      ? description.slice(0, CODEX_DESCRIPTION_CAP - 3).join("") + "..."
      : input.description;
    const allocation = codexExampleAllocation(names, capped, budget);
    needed = allocation.needed;
    states = allocation.kept.map((characters) => {
      if (characters === null) return "dropped";
      if (characters === 0) return "nameOnly";
      return characters < description.length ? "shortened" : "full";
    });
  }
  const counts = { full: 0, shortened: 0, nameOnly: 0, dropped: 0 };
  for (const state of states) counts[state] += 1;
  return { ...budget, needed, states, counts };
}
