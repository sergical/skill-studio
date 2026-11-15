// ============================================================================
// Skill Studio - Skill pile data
// The made-up skill pile the homepage hero throws around: 92 skills in 11
// folders, with a fixed seed so the prerender and every visit draw the same pile
// ============================================================================

export type PileState = "ok" | "outdated" | "broken" | "unused" | "copy";

export interface PileChip {
  id: string;
  name: string;
  folder: string;
  state: PileState;
}

const FOLDERS = [
  "~/.claude",
  "~/.codex",
  "~/.agents",
  "~/.cursor",
  "opencode",
  "vercel plugin",
  "sentry plugin",
  "acme-web",
  "acme-api",
  "design-system",
  "old-prototype",
];

const KEPT: ReadonlyArray<[name: string, state: PileState]> = [
  ["agent-browser", "ok"],
  ["frontend-design", "ok"],
  ["motion", "broken"],
  ["tailwind", "ok"],
  ["commit", "ok"],
  ["skill-creator", "outdated"],
  ["prepare-videos", "ok"],
  ["release-notes", "ok"],
  ["code-review", "broken"],
  ["pdf-tools", "outdated"],
  ["deploy-preview", "ok"],
  ["writing-skills", "ok"],
  ["sentry-debug", "ok"],
  ["db-migrate", "outdated"],
  ["changelog", "broken"],
  ["playwright", "ok"],
];

const UNUSED = [
  "pr-writer",
  "api-docs",
  "seo-audit",
  "excel-reports",
  "slack-gif",
  "brand-voice",
  "docx",
  "pptx",
  "mcp-builder",
  "webapp-testing",
  "canvas-design",
  "theme-factory",
  "internal-comms",
  "react-native",
  "expo-router",
  "swiftui",
  "terraform",
  "k8s-debug",
  "postgres-tuning",
  "graphql-schema",
  "storybook",
  "cypress",
  "vitest-setup",
  "eslint-config",
  "prisma",
  "supabase",
  "stripe-checkout",
  "auth-flows",
  "i18n",
  "a11y-audit",
  "figma-handoff",
  "linear-triage",
];

const COPY_COUNT = 44;

// A fixed seed, so the prerender and every visit draw the same pile.
function seededRandom(seed: number) {
  let state = seed;
  return () => {
    state = (state + 0x6d2b79f5) | 0;
    let t = Math.imul(state ^ (state >>> 15), 1 | state);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function buildPile(): PileChip[] {
  const random = seededRandom(455);
  const pick = <T>(items: ReadonlyArray<T>) => items[Math.floor(random() * items.length)];
  const entries: Array<Pick<PileChip, "name" | "folder" | "state">> = [
    ...KEPT.map(([name, state]) => ({ name, state, folder: pick(FOLDERS.slice(0, 3)) })),
    ...UNUSED.map((name) => ({ name, state: "unused" as const, folder: pick(FOLDERS) })),
  ];
  const originals = [...entries];
  for (let index = 0; index < COPY_COUNT; index += 1) {
    const original = pick(originals);
    const folder = pick(FOLDERS.filter((candidate) => candidate !== original.folder));
    entries.push({ name: original.name, folder, state: "copy" });
  }
  // Shuffled, so kept skills are spread through the pile instead of on top of it.
  for (let index = entries.length - 1; index > 0; index -= 1) {
    const swap = Math.floor(random() * (index + 1));
    [entries[index], entries[swap]] = [entries[swap], entries[index]];
  }
  return entries.map((entry, index) => ({
    ...entry,
    id: `${entry.name}@${entry.folder}#${index}`,
  }));
}

export const PILE = buildPile();
export const isKept = (chip: PileChip) =>
  chip.state === "ok" || chip.state === "broken" || chip.state === "outdated";
export const KEPT_CHIPS = PILE.filter(isKept);
const countState = (state: PileState) => PILE.filter((chip) => chip.state === state).length;

export const PILE_COUNTS = {
  total: PILE.length,
  folders: FOLDERS.length,
  copies: countState("copy"),
  unused: countState("unused"),
  broken: countState("broken"),
  outdated: countState("outdated"),
  kept: KEPT_CHIPS.length,
  parked: PILE.length - KEPT_CHIPS.length,
};

export const STATE_COLOR = {
  ok: "oklch(0.75 0.17 150)",
  outdated: "oklch(0.72 0.15 240)",
  broken: "oklch(0.66 0.21 25)",
  unused: "oklch(0.6 0.02 290)",
  copy: "oklch(0.8 0.15 80)",
} as const satisfies Record<PileState, string>;
