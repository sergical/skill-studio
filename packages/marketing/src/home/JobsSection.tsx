import * as stylex from "@stylexjs/stylex";
import { ArrowDownRight } from "lucide-react";

import { AgentIcon, type AgentId } from "../AgentIcon";
import { siteTokens } from "../SiteTheme.stylex";
import { homeSectionStyles } from "./HomeSection.stylex";

const SUPPORTED_AGENTS = [
  { id: "claude", name: "Claude Code" },
  { id: "codex", name: "Codex" },
  { id: "cursor", name: "Cursor" },
  { id: "opencode", name: "OpenCode" },
  { id: "pi", name: "pi" },
  { id: "grok", name: "Grok Build" },
] satisfies ReadonlyArray<{ id: AgentId; name: string }>;

const JOB_GROUPS = [
  {
    title: "See and clean up",
    jobs: [
      {
        label: "Find my skills",
        how: "Browse installed skills across the agents and projects you choose.",
        link: "#product",
      },
      {
        label: "Clean up my skills",
        how: "Fix broken skills and park or remove unused ones, with undo.",
        link: "#product",
      },
    ],
  },
  {
    title: "Install and update",
    jobs: [
      {
        label: "Add skills",
        how: "Pick a skill or pack and choose which agents get it.",
        link: "#install-skills",
      },
      {
        label: "Update my skills",
        how: "Find and apply updates for skills and packs.",
        link: "#update-skills",
      },
    ],
  },
  {
    title: "Customize and share",
    jobs: [
      {
        label: "Choose per agent",
        how: "Keep a skill on in Claude Code and off in Codex.",
        link: "#choose-skills-per-agent",
      },
      {
        label: "Make a skill mine",
        how: "Edit a fork of a shared skills.sh skill. Pull changes from the original.",
        link: "#fork-skills",
      },
      {
        label: "Share a skill pack",
        how: "Export selected skills as a pack to share.",
        link: "#share-skills",
      },
    ],
  },
  {
    title: "Work from your agent",
    jobs: [
      {
        label: "Ask my agent",
        how: "Use MCP to ask your agent to check and park skills, or use the CLI.",
        link: "#use-it",
      },
    ],
  },
] as const;

/** Links the most common skill jobs to the demos and steps below. */
export function JobsSection() {
  return (
    <>
      <div {...stylex.props(styles.intro)}>
        <h2 {...stylex.props(homeSectionStyles.title, styles.title)}>What you can do</h2>
        <div {...stylex.props(styles.agents)}>
          <span {...stylex.props(styles.agentsLabel)}>Works with</span>
          <ul aria-label="Supported agents" {...stylex.props(styles.agentList)}>
            {SUPPORTED_AGENTS.map(({ id, name }) => (
              <li key={id} {...stylex.props(styles.agent)}>
                <span aria-hidden="true" {...stylex.props(styles.agentIcon)}>
                  <AgentIcon agent={id} size={18} />
                </span>
                {name}
              </li>
            ))}
          </ul>
        </div>
      </div>
      <div {...stylex.props(styles.groups)}>
        {JOB_GROUPS.map(({ title, jobs }) => (
          <div key={title} {...stylex.props(styles.group)}>
            <h3 {...stylex.props(styles.groupTitle)}>{title}</h3>
            <ul {...stylex.props(styles.jobs)}>
              {jobs.map(({ label, how, link }) => (
                <li key={label}>
                  <a href={link} {...stylex.props(styles.job)}>
                    <span {...stylex.props(styles.jobLabel)}>
                      {label}
                      <ArrowDownRight
                        aria-hidden="true"
                        size={14}
                        {...stylex.props(styles.arrow)}
                      />
                    </span>
                    <span {...stylex.props(styles.jobHow)}>{how}</span>
                  </a>
                </li>
              ))}
            </ul>
          </div>
        ))}
      </div>
    </>
  );
}

const styles = stylex.create({
  intro: {
    alignItems: "baseline",
    display: "flex",
    flexWrap: "wrap",
    gap: "18px 38px",
    justifyContent: "space-between",
    marginBottom: 24,
  },
  title: { margin: 0 },
  agents: {
    alignItems: "center",
    display: "flex",
    flexWrap: "wrap",
    gap: "8px 18px",
  },
  agentsLabel: { color: siteTokens.muted, fontSize: 12 },
  agentList: {
    display: "flex",
    flexWrap: "wrap",
    gap: "8px 18px",
    listStyle: "none",
    margin: 0,
    padding: 0,
  },
  agent: {
    alignItems: "center",
    color: siteTokens.text,
    display: "flex",
    fontSize: 13,
    gap: 6,
    whiteSpace: "nowrap",
  },
  agentIcon: { display: "flex" },
  groups: {
    display: "grid",
    gap: 24,
    gridTemplateColumns: "repeat(4, minmax(0, 1fr))",
    "@media (max-width: 940px)": { gridTemplateColumns: "repeat(2, minmax(0, 1fr))" },
    "@media (max-width: 600px)": { gap: 18 },
  },
  group: { borderTop: `1px solid ${siteTokens.border}`, paddingTop: 12 },
  groupTitle: { color: siteTokens.muted, fontSize: 13, fontWeight: 560, margin: "0 0 8px" },
  jobs: { listStyle: "none", margin: 0, padding: 0 },
  job: {
    color: { default: siteTokens.text, ":hover": siteTokens.accent },
    display: "grid",
    gap: 4,
    minHeight: 44,
    paddingBlock: 8,
    textDecoration: "none",
    ":focus-visible": { outline: `2px solid ${siteTokens.accent}`, outlineOffset: 3 },
  },
  jobLabel: { fontSize: 16, fontWeight: 640, lineHeight: 1.35 },
  arrow: { marginInlineStart: 5, verticalAlign: "middle" },
  jobHow: { color: siteTokens.muted, fontSize: 14, lineHeight: 1.45 },
});
