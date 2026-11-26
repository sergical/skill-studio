// ============================================================================
// Skill Studio - Skill pile data
// A fictional set of skills and folders, with a fixed seed so the prerender
// and every visit draw the same pile.
// The sorted scene keeps the skills in use and parks the rest.
// ============================================================================

export type PileSkillStatus = "fine" | "warning" | "broken";

export interface PileChip {
  id: string;
  name: string;
  folder: string;
  inUse: boolean;
  status: PileSkillStatus;
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

const IN_USE = [
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

// Copies of a skill in another folder.
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
  type PileEntry = Omit<PileChip, "id">;
  const entries = [
    ...IN_USE.map<PileEntry>((name, index) => ({
      name,
      inUse: true,
      status: index < 4 ? "broken" : index < 7 ? "warning" : "fine",
      folder: pick(FOLDERS.slice(0, 3)),
    })),
    ...UNUSED.map<PileEntry>((name, index) => ({
      name,
      inUse: false,
      folder: pick(FOLDERS),
      status: index % 7 === 0 ? "broken" : index % 3 === 0 ? "warning" : "fine",
    })),
  ];
  const originals = [...entries];
  for (let index = 0; index < COPY_COUNT; index += 1) {
    const original = pick(originals);
    const folder = pick(FOLDERS.filter((candidate) => candidate !== original.folder));
    entries.push({
      name: original.name,
      folder,
      inUse: false,
      status: original.status,
    });
  }
  // Shuffle so skills in use are spread through the pile.
  for (let index = entries.length - 1; index > 0; index -= 1) {
    const swap = Math.floor(random() * (index + 1));
    [entries[index], entries[swap]] = [entries[swap], entries[index]];
  }
  return entries.map((entry, index) => ({
    ...entry,
    id: `${entry.name}@${entry.folder}#${index}`,
  }));
}

/** The fictional library shown in the homepage hero. */
export const PILE = buildPile();
/** The skills the mascot keeps in the active list. */
export const ACTIVE_PILE_CHIPS = PILE.filter((chip) => chip.inUse);

/** The actions shown in the mascot’s cleanup. */
export const PILE_CLEANUP = {
  parked: PILE.length - ACTIVE_PILE_CHIPS.length,
  fixed: ACTIVE_PILE_CHIPS.filter((chip) => chip.status === "broken").length,
  updated: ACTIVE_PILE_CHIPS.filter((chip) => chip.status === "warning").length,
};
