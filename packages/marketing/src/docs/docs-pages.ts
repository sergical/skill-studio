import { fillAgentSetup } from "./agent-setup-markdown";

const markdownFiles = import.meta.glob<string>("./content/*.md", {
  eager: true,
  query: "?raw",
  import: "default",
});

const docsNavigation = {
  Start: [
    [
      "get-started",
      "Start here",
      "Choose a surface and scan your skills.",
      "Get started with Skill Studio",
    ],
    ["requirements", "Requirements and install", "Check what your Mac needs for each job."],
  ],
  Jobs: [
    ["inventory", "See every skill", "Find skills, sources, agents and copies."],
    ["cleanup", "Clean up and undo", "Diagnose, repair, park, remove and restore skills."],
    ["install-pack", "Install a pack", "Add skills from a Git repository."],
    ["update", "Update skills and packs", "Check for changes and update installed skills."],
    [
      "one-agent",
      "Turn a skill on or off for one agent",
      "Change one agent’s copy and keep the others active.",
    ],
    ["fork", "Fork and pull upstream", "Keep local edits and bring in changes from the source."],
    ["export-pack", "Export a pack", "Copy selected skills into a shareable folder."],
  ],
  Agents: [
    ["agent-setup", "Set up with an agent", "Connect the MCP server and run your first check."],
    ["usage", "Check skill usage", "Read the available history from the last 30 days."],
    ["listing", "Check listing budgets", "See which descriptions fit in the model’s skill list."],
  ],
  Reference: [
    ["cli", "CLI", "Find terminal commands and options.", "Skill Studio CLI"],
    ["mcp", "MCP server", "Find server setup and tools.", "Skill Studio MCP server"],
  ],
  Help: [
    ["privacy", "Privacy and telemetry", "See what stays on your Mac and what goes online."],
    [
      "troubleshooting",
      "Troubleshooting",
      "Recover from common scan, install and startup failures.",
    ],
  ],
} as const;

export interface DocsPageEntry {
  slug: string;
  path: string;
  markdownPath: string;
  navLabel: string;
  title: string;
  description: string;
  markdown: string;
}

export const DOCS_GROUPS = Object.entries(docsNavigation).map(([group, entries]) => ({
  group,
  pages: entries.map(([slug, navLabel, description, title]): DocsPageEntry => {
    const source = markdownFiles[`./content/${slug}.md`];
    if (!source) throw new Error(`Docs navigation has no Markdown source for ${slug}`);
    return {
      slug,
      path: slug === "get-started" ? "/docs/" : `/docs/${slug}/`,
      markdownPath: slug === "get-started" ? "docs/index.md" : `docs/${slug}.md`,
      navLabel,
      title: title ?? `${navLabel} | Skill Studio`,
      description,
      markdown: fillAgentSetup(source),
    };
  }),
}));

export const DOCS_PAGES = DOCS_GROUPS.flatMap(({ pages }) => pages);
