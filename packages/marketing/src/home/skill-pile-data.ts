// ============================================================================
// Skill Studio - Skill pile data
// The made-up skill pile the homepage hero throws around: 92 skills in 11
// folders, with a fixed seed so the prerender and every visit draw the same pile.
// The sorted scene keeps the skills in use and parks the rest.
// ============================================================================

export interface PileChip {
  id: string;
  name: string;
  folder: string;
  inUse: boolean;
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
  const entries: Array<Pick<PileChip, "name" | "folder" | "inUse">> = [
    ...IN_USE.map((name) => ({
      name,
      inUse: true,
      folder: pick(FOLDERS.slice(0, 3)),
    })),
    ...UNUSED.map((name) => ({ name, inUse: false, folder: pick(FOLDERS) })),
  ];
  const originals = [...entries];
  for (let index = 0; index < COPY_COUNT; index += 1) {
    const original = pick(originals);
    const folder = pick(FOLDERS.filter((candidate) => candidate !== original.folder));
    entries.push({
      name: original.name,
      folder,
      inUse: false,
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

/** The example scan totals shown with the skill pile. */
export const PILE_COUNTS = {
  total: PILE.length,
  folders: FOLDERS.length,
  active: ACTIVE_PILE_CHIPS.length,
  parked: PILE.length - ACTIVE_PILE_CHIPS.length,
};
