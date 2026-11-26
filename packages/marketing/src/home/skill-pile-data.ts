// ============================================================================
// Skill Studio - Skill pile data
// The made-up skill pile the homepage hero throws around: 92 skills in 11
// folders, with a fixed seed so the prerender and every visit draw the same pile.
// Each skill has the state an agent's model sees it in. The three counts keep
// the ratio measured on one machine (Claude Code, 1% listing budget): 49
// described, 107 name only, 129 hidden.
// ============================================================================

export type PileState = "described" | "name-only" | "hidden";

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

const DESCRIBED = [
  "agent-browser",
  "frontend-design",
  "motion",
  "tailwind",
  "commit",
  "skill-creator",
  "prepare-videos",
  "release-notes",
  "code-review",
  "pdf-tools",
  "deploy-preview",
  "writing-skills",
  "sentry-debug",
  "db-migrate",
  "changelog",
  "playwright",
];

const NAME_ONLY = [
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

// Copies of a skill in another folder. Most are hidden: an agent does not list a skill that
// is manual-only, turned off, or installed for another project.
const COPY_COUNT = 44;
const NAME_ONLY_COPIES = 3;

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
    ...DESCRIBED.map((name) => ({
      name,
      state: "described" as const,
      folder: pick(FOLDERS.slice(0, 3)),
    })),
    ...NAME_ONLY.map((name) => ({ name, state: "name-only" as const, folder: pick(FOLDERS) })),
  ];
  const originals = [...entries];
  for (let index = 0; index < COPY_COUNT; index += 1) {
    const original = pick(originals);
    const folder = pick(FOLDERS.filter((candidate) => candidate !== original.folder));
    entries.push({
      name: original.name,
      folder,
      state: index < NAME_ONLY_COPIES ? "name-only" : "hidden",
    });
  }
  // Shuffled, so described skills are spread through the pile instead of on top of it.
  for (let index = entries.length - 1; index > 0; index -= 1) {
    const swap = Math.floor(random() * (index + 1));
    [entries[index], entries[swap]] = [entries[swap], entries[index]];
  }
  return entries.map((entry, index) => ({
    ...entry,
    id: `${entry.name}@${entry.folder}#${index}`,
  }));
}

const PILE = buildPile();
const DESCRIBED_CHIPS = PILE.filter((chip) => chip.state === "described");
const countState = (state: PileState) => PILE.filter((chip) => chip.state === state).length;

export const PILE_COUNTS = {
  total: PILE.length,
  folders: FOLDERS.length,
  described: DESCRIBED_CHIPS.length,
  nameOnly: countState("name-only"),
  hidden: countState("hidden"),
  notDescribed: PILE.length - DESCRIBED_CHIPS.length,
};

export const STATE_COLOR = {
  described: "oklch(0.75 0.17 150)",
  "name-only": "oklch(0.8 0.15 80)",
  hidden: "oklch(0.6 0.02 290)",
} as const satisfies Record<PileState, string>;
