// ============================================================================
// Skill Studio - First scan steps
// The desktop app's first run as three steps: pick agents, find folders, scan
// and tidy. It answers "how does it know where my skills are" before download.
// ============================================================================
import type { ReactNode } from "react";
import * as stylex from "@stylexjs/stylex";
import { Check, FolderOpen, History } from "lucide-react";

import { AgentIcon, type AgentId } from "../AgentIcon";
import { siteTokens } from "../SiteTheme.stylex";
import { homeSectionStyles } from "./HomeSection.stylex";
import { PILE_COUNTS } from "./skill-pile-data";

// Row states and wording match apps/desktop FirstRunScreen.
const AGENT_ROWS = [
  { id: "claude", name: "Claude Code", note: null, kept: true },
  { id: "codex", name: "Codex", note: null, kept: true },
  { id: "cursor", name: "Cursor", note: "No activity yet", kept: true },
  { id: "opencode", name: "OpenCode", note: null, kept: true },
  { id: "pi", name: "pi", note: "Not found", kept: false },
  { id: "grok", name: "Grok Build", note: "Not found", kept: false },
] satisfies ReadonlyArray<{ id: AgentId; name: string; note: string | null; kept: boolean }>;

const FOLDER_ROWS = [
  { path: "~/.claude/skills", source: "Claude Code" },
  { path: "~/.codex/skills", source: "Codex" },
  { path: "~/.agents/skills", source: "shared" },
  { path: "~/src/acme-web", source: "history" },
  { path: "~/src/acme-api", source: "history" },
  { path: "~/work/design-system", source: "history" },
] as const;

const SCAN_ROWS = [
  { label: "skills found", value: PILE_COUNTS.total, color: "oklch(0.75 0.17 150)" },
  { label: "copies of one skill", value: PILE_COUNTS.copies, color: "oklch(0.8 0.15 80)" },
  { label: "not used in 30 days", value: PILE_COUNTS.unused, color: "oklch(0.6 0.02 290)" },
  { label: "broken", value: PILE_COUNTS.broken, color: "oklch(0.66 0.21 25)" },
] as const;

function Step({
  number,
  title,
  copy,
  children,
}: {
  number: number;
  title: string;
  copy: string;
  children: ReactNode;
}) {
  return (
    <li {...stylex.props(styles.step)}>
      <div {...stylex.props(styles.visual)} aria-hidden="true">
        {children}
      </div>
      <span {...stylex.props(styles.stepNumber)}>Step {number}</span>
      <h3 {...stylex.props(styles.stepTitle)}>{title}</h3>
      <p {...stylex.props(styles.stepCopy)}>{copy}</p>
    </li>
  );
}

function AgentRows() {
  return (
    <ul {...stylex.props(styles.rows)}>
      {AGENT_ROWS.map((row) => (
        <li key={row.id} {...stylex.props(styles.row, !row.kept && styles.rowOff)}>
          <span {...stylex.props(styles.checkbox, row.kept && styles.checkboxOn)}>
            {row.kept && <Check size={10} strokeWidth={3} />}
          </span>
          <AgentIcon agent={row.id} size={14} />
          <span {...stylex.props(styles.rowLabel)}>{row.name}</span>
          {row.note && <span {...stylex.props(styles.rowNote)}>{row.note}</span>}
        </li>
      ))}
    </ul>
  );
}

function FolderRows() {
  return (
    <>
      <ul {...stylex.props(styles.rows)}>
        {FOLDER_ROWS.map((row) => (
          <li key={row.path} {...stylex.props(styles.row)}>
            {row.source === "history" ? (
              <History size={13} {...stylex.props(styles.rowIcon)} />
            ) : (
              <FolderOpen size={13} {...stylex.props(styles.rowIcon)} />
            )}
            <span {...stylex.props(styles.rowLabel, styles.mono)}>{row.path}</span>
            <span {...stylex.props(styles.rowNote)}>{row.source}</span>
          </li>
        ))}
      </ul>
      <div {...stylex.props(styles.switchRow)}>
        <span>Find my projects from agent history</span>
        <span {...stylex.props(styles.switchTrack)}>
          <span {...stylex.props(styles.switchThumb)} />
        </span>
      </div>
    </>
  );
}

function ScanRows() {
  return (
    <>
      <ul {...stylex.props(styles.rows)}>
        {SCAN_ROWS.map((row) => (
          <li key={row.label} {...stylex.props(styles.row)}>
            <span {...stylex.props(styles.dot, styles.dotColor(row.color))} />
            <span {...stylex.props(styles.scanValue)}>{row.value}</span>
            <span {...stylex.props(styles.rowLabel)}>{row.label}</span>
          </li>
        ))}
      </ul>
      <div {...stylex.props(styles.parkRow)}>
        <span>
          Park {PILE_COUNTS.parked} · keep {PILE_COUNTS.kept}
        </span>
        <span {...stylex.props(styles.undo)}>Undo</span>
      </div>
    </>
  );
}

export function FirstScanSteps() {
  return (
    <div>
      <h2 {...stylex.props(homeSectionStyles.title, styles.title)}>How it works.</h2>
      <div {...stylex.props(homeSectionStyles.columns)}>
        <div {...stylex.props(styles.intro)}>
          <p {...stylex.props(styles.lead)}>
            Skill Studio looks only where agents keep skills. You choose those places on first
            launch.
          </p>
          <ul {...stylex.props(styles.promises)}>
            <li>It does not search the rest of your disk.</li>
            <li>Park moves a skill away from every agent. Undo puts it back.</li>
            <li>It runs on your Mac. Free and open source.</li>
          </ul>
        </div>
        <ol {...stylex.props(styles.steps)}>
          <Step
            number={1}
            title="Pick your agents."
            copy="Skill Studio finds the agents on your Mac. Keep the ones it should manage."
          >
            <AgentRows />
          </Step>
          <Step
            number={2}
            title="It finds your folders."
            copy="Global folders come from your agents. Project folders come from agent history. Add or remove any folder in Settings."
          >
            <FolderRows />
          </Step>
          <Step
            number={3}
            title="Scan, then tidy."
            copy="See every skill in one list. Fix the broken ones and park the copies and the unused."
          >
            <ScanRows />
          </Step>
        </ol>
      </div>
    </div>
  );
}

const styles = stylex.create({
  title: { letterSpacing: "-.03em" },
  intro: { display: "flex", flexDirection: "column", gap: 20 },
  lead: {
    color: siteTokens.text,
    fontSize: 17,
    lineHeight: 1.55,
    margin: 0,
    textWrap: "pretty",
  },
  promises: {
    color: siteTokens.muted,
    display: "flex",
    flexDirection: "column",
    fontSize: 14,
    gap: 10,
    lineHeight: 1.5,
    listStyle: "none",
    margin: 0,
    padding: 0,
    "@media (max-width: 760px)": { marginBottom: 32 },
  },
  steps: {
    display: "grid",
    gap: 16,
    gridTemplateColumns: "repeat(3, minmax(0, 1fr))",
    listStyle: "none",
    margin: 0,
    padding: 0,
    "@media (max-width: 1100px)": { gridTemplateColumns: "1fr" },
  },
  step: { display: "flex", flexDirection: "column", minWidth: 0 },
  visual: {
    backgroundColor: siteTokens.surface,
    borderColor: siteTokens.border,
    borderRadius: 14,
    borderStyle: "solid",
    borderWidth: 1,
    display: "flex",
    flexDirection: "column",
    gap: 10,
    marginBottom: 18,
    minHeight: 236,
    padding: 14,
  },
  stepNumber: {
    color: siteTokens.accent,
    fontSize: 12,
    fontWeight: 620,
    marginBottom: 6,
  },
  stepTitle: { fontSize: 17, fontWeight: 640, letterSpacing: "-.01em", margin: "0 0 8px" },
  stepCopy: {
    color: siteTokens.muted,
    fontSize: 14,
    lineHeight: 1.55,
    margin: 0,
    textWrap: "pretty",
  },
  rows: {
    borderColor: siteTokens.border,
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    listStyle: "none",
    margin: 0,
    overflow: "hidden",
    padding: 0,
  },
  row: {
    alignItems: "center",
    borderBottomColor: siteTokens.border,
    borderBottomStyle: { default: "solid", ":last-child": "none" },
    borderBottomWidth: 1,
    display: "flex",
    fontSize: 12,
    gap: 8,
    height: 30,
    paddingInline: 10,
  },
  rowOff: { opacity: 0.5 },
  rowIcon: { color: siteTokens.muted, flexShrink: 0 },
  rowLabel: {
    color: siteTokens.text,
    flex: 1,
    minWidth: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  mono: { fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace", fontSize: 11 },
  rowNote: { color: siteTokens.muted, flexShrink: 0, fontSize: 11 },
  checkbox: {
    alignItems: "center",
    borderColor: siteTokens.border,
    borderRadius: 4,
    borderStyle: "solid",
    borderWidth: 1,
    color: siteTokens.accentText,
    display: "inline-flex",
    flexShrink: 0,
    height: 14,
    justifyContent: "center",
    width: 14,
  },
  checkboxOn: { backgroundColor: siteTokens.accent, borderColor: siteTokens.accent },
  switchRow: {
    alignItems: "center",
    color: siteTokens.text,
    display: "flex",
    fontSize: 12,
    justifyContent: "space-between",
    paddingInline: 4,
  },
  switchTrack: {
    backgroundColor: siteTokens.accent,
    borderRadius: 999,
    display: "inline-flex",
    height: 16,
    justifyContent: "flex-end",
    padding: 2,
    width: 28,
  },
  switchThumb: { backgroundColor: siteTokens.accentText, borderRadius: 999, height: 12, width: 12 },
  dot: { borderRadius: 999, flexShrink: 0, height: 7, width: 7 },
  dotColor: (color: string) => ({ backgroundColor: color }),
  scanValue: {
    fontVariantNumeric: "tabular-nums",
    fontWeight: 640,
    minWidth: 22,
    textAlign: "right",
  },
  parkRow: {
    alignItems: "center",
    backgroundColor: siteTokens.accentSoft,
    borderRadius: 8,
    color: siteTokens.text,
    display: "flex",
    fontSize: 12,
    fontWeight: 560,
    height: 34,
    justifyContent: "space-between",
    marginTop: "auto",
    paddingInline: 12,
  },
  undo: { color: siteTokens.muted, fontWeight: 500 },
});
