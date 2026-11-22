import * as stylex from "@stylexjs/stylex";
import { Check, Link2Off, RefreshCw, TriangleAlert } from "lucide-react";

import { AgentIcon, type AgentId } from "../AgentIcon";
import { productDemoLightTheme, productDemoTokens } from "../ProductDemoTokens.stylex";
import { siteTokens, type SiteTheme } from "../SiteTheme.stylex";
import { homeSectionStyles } from "./HomeSection.stylex";

const AGENTS = [
  ["claude", "Claude Code"],
  ["codex", "Codex"],
  ["cursor", "Cursor"],
  ["opencode", "OpenCode"],
  ["pi", "pi"],
  ["grok", "Grok Build"],
] as const satisfies ReadonlyArray<readonly [AgentId, string]>;

/** Shows the scan, the cleanup actions, and the listing problem in three app views. */
export function CleanupOverview({ theme }: { theme: SiteTheme }) {
  return (
    <>
      <h2 {...stylex.props(homeSectionStyles.title, styles.title)}>
        Find and clean up your skills
      </h2>
      <ul aria-label="Supported agents" {...stylex.props(styles.agents)}>
        {AGENTS.map(([id, name]) => (
          <li key={id} {...stylex.props(styles.agent)}>
            <span aria-hidden="true">
              <AgentIcon agent={id} size={18} />
            </span>
            {name}
          </li>
        ))}
      </ul>
      <div {...stylex.props(styles.views, theme === "light" && productDemoLightTheme)}>
        <article>
          <h3 {...stylex.props(styles.heading)}>See what’s installed</h3>
          <p {...stylex.props(styles.copy)}>
            Find skills in your agents’ home and project folders. See what’s fine, broken, or
            different across copies.
          </p>
          <ul aria-hidden="true" {...stylex.props(styles.preview)}>
            <li {...stylex.props(styles.row)}>
              <Check size={15} {...stylex.props(styles.fine)} /> commit{" "}
              <span {...stylex.props(styles.detail)}>Ready</span>
            </li>
            <li {...stylex.props(styles.row)}>
              <Link2Off size={15} {...stylex.props(styles.broken)} /> release-notes{" "}
              <span {...stylex.props(styles.detail)}>Broken link</span>
            </li>
            <li {...stylex.props(styles.row)}>
              <TriangleAlert size={15} {...stylex.props(styles.warning)} /> motion{" "}
              <span {...stylex.props(styles.detail)}>Copies differ</span>
            </li>
          </ul>
        </article>
        <article>
          <h3 {...stylex.props(styles.heading)}>Fix, park, and update</h3>
          <p {...stylex.props(styles.copy)}>
            Repair broken links, set aside unused skills, and update supported installs. Check
            Activity for undo.
          </p>
          <ul aria-hidden="true" {...stylex.props(styles.preview)}>
            <li {...stylex.props(styles.row)}>
              release-notes <span {...stylex.props(styles.action)}>Fix</span>
            </li>
            <li {...stylex.props(styles.row)}>
              deploy-preview <span {...stylex.props(styles.action)}>Park</span>
            </li>
            <li {...stylex.props(styles.row)}>
              agent-browser <span {...stylex.props(styles.action)}>Update</span>
            </li>
          </ul>
        </article>
        <article>
          <h3 {...stylex.props(styles.heading)}>See what fits</h3>
          <p {...stylex.props(styles.copy)}>
            See when an agent has more skills than it has room to describe in its context.
          </p>
          <ul aria-hidden="true" {...stylex.props(styles.preview)}>
            <li {...stylex.props(styles.row)}>
              Model listing <RefreshCw size={14} {...stylex.props(styles.detail)} />
            </li>
            <li {...stylex.props(styles.row)}>
              commit <span {...stylex.props(styles.detail)}>Full description</span>
            </li>
            <li {...stylex.props(styles.row)}>
              motion <span {...stylex.props(styles.detail)}>Shortened</span>
            </li>
          </ul>
        </article>
      </div>
    </>
  );
}

const styles = stylex.create({
  title: { marginBottom: 12 },
  agents: {
    display: "flex",
    flexWrap: "wrap",
    gap: "8px 18px",
    listStyle: "none",
    margin: "0 0 30px",
    padding: 0,
  },
  agent: { alignItems: "center", display: "flex", fontSize: 13, gap: 6, whiteSpace: "nowrap" },
  views: {
    display: "grid",
    gap: 28,
    gridTemplateColumns: "repeat(3, minmax(0, 1fr))",
    "@media (max-width: 760px)": { gridTemplateColumns: "minmax(0, 1fr)" },
  },
  heading: { fontSize: 19, fontWeight: 640, margin: "0 0 7px" },
  copy: {
    color: siteTokens.muted,
    fontSize: 14,
    lineHeight: 1.55,
    margin: "0 0 12px",
    minHeight: 44,
  },
  preview: {
    backgroundColor: siteTokens.surface,
    border: `1px solid ${siteTokens.border}`,
    borderRadius: 10,
    listStyle: "none",
    margin: 0,
    padding: "2px 12px",
  },
  row: {
    alignItems: "center",
    display: "flex",
    fontSize: 13,
    gap: 8,
    minHeight: 36,
    ":not(:last-child)": { borderBottom: `1px solid ${siteTokens.border}` },
  },
  detail: { color: siteTokens.muted, marginInlineStart: "auto" },
  action: {
    border: `1px solid ${siteTokens.border}`,
    borderRadius: 5,
    marginInlineStart: "auto",
    padding: "1px 6px",
  },
  fine: { color: "oklch(0.696 0.149 162.48)" },
  broken: { color: productDemoTokens.danger },
  warning: { color: productDemoTokens.warning },
});
