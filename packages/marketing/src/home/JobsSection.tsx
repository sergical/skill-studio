// ============================================================================
// Skill Studio - Jobs section
// Four everyday skill jobs, each shown by hand and then with Skill Studio.
// Pack names are fictional; every command and app action is real.
// ============================================================================
import { useState, type KeyboardEvent } from "react";
import * as stylex from "@stylexjs/stylex";
import {
  Check,
  FolderSearch,
  PackageOpen,
  Stethoscope,
  Users,
  type LucideIcon,
} from "lucide-react";

import { siteTokens } from "../SiteTheme.stylex";
import { homeSectionStyles } from "./HomeSection.stylex";

type JobId = "update" | "share" | "broken" | "unused";

interface Step {
  text: string;
  code?: string;
}

interface Job {
  id: JobId;
  icon: LucideIcon;
  label: string;
  blurb: string;
  byHand: Step[];
  byHandCost: string;
  withStudio: Step[];
  command: string;
}

const jobs: ReadonlyArray<Job> = [
  {
    id: "update",
    icon: PackageOpen,
    label: "A skill pack has a new version",
    blurb: "Update every copy, not only one.",
    byHand: [
      { text: "Update the shared copy.", code: "npx skills update" },
      {
        text: "Find the copies you made for your other agents.",
        code: "ls ~/.codex/skills ~/.cursor/skills ~/.config/opencode/skills",
      },
      {
        text: "Compare each copy with the new version.",
        code: "diff -r ~/.agents/skills/sql-review ~/.cursor/skills/sql-review",
      },
      { text: "Copy the new files into each agent's folder. Do it again for the next skill." },
    ],
    byHandCost: "One folder per agent. Miss one and that agent keeps the old version.",
    withStudio: [
      { text: "Updates on Home lists each skill that has a new version." },
      { text: "Click Pull latest, or Update all." },
      { text: "Warnings shows each copy that still differs. Compare the copies side by side." },
    ],
    command: "skill-studio outdated",
  },
  {
    id: "share",
    icon: Users,
    label: "A teammate asks for your setup",
    blurb: "Know where each skill came from.",
    byHand: [
      {
        text: "Read the lock file to find the repo of each skill.",
        code: "cat ~/.agents/.skill-lock.json",
      },
      { text: "Look in each agent's folder for skills that are not in the lock file." },
      { text: "Guess which skills you wrote and which ones you changed." },
      { text: "Paste a list of repos into chat. Zip the rest." },
    ],
    byHandCost: "The lock file knows only one installer. Your own skills are not in it.",
    withStudio: [
      {
        text: "Each skill shows its source: a repo such as acme-labs/review-skills, a plugin, or a folder you made.",
      },
      {
        text: "Add skills to a project with dotagents. Commit agents.toml, and your team gets the same skills.",
      },
    ],
    command:
      "skill-studio add acme-labs/review-skills --name sql-review --method dotagents --project-path .",
  },
  {
    id: "broken",
    icon: Stethoscope,
    label: "Your agent ignores a skill",
    blurb: "Find out why in one check.",
    byHand: [
      {
        text: "Check that the skill is in a folder this agent reads. Claude Code does not read .agents/skills.",
      },
      {
        text: "Open SKILL.md and check the header. A bad header can stop the agent from loading the skill.",
      },
      {
        text: "Follow the link. The folder it points at can be gone.",
        code: "readlink ~/.claude/skills/pdf-tools",
      },
    ],
    byHandCost:
      "Each agent reads different folders, and none of them tells you why it skipped a skill.",
    withStudio: [
      { text: "Diagnose names the problem in each skill, for each agent." },
      { text: "Fix repairs the header or the link, and lists what it cannot repair." },
      { text: "Undo reverses the fix." },
    ],
    command: "skill-studio diagnose",
  },
  {
    id: "unused",
    icon: FolderSearch,
    label: "Your agents load skills you never use",
    blurb: "Keep only the skills that run.",
    byHand: [
      { text: "Each skill's description goes into your agent's prompt, used or not." },
      {
        text: "Search each agent's session logs to find which skills ran. Each agent uses a different log format.",
      },
      { text: "Delete the folders you think you do not need, and hope that you are right." },
    ],
    byHandCost: "No agent shows which skills it used, so the list only gets longer.",
    withStudio: [
      { text: "Usage shows which skills ran in the last 30 days, for each agent." },
      { text: "Park the rest. They leave your agents' lists. Nothing is deleted." },
      { text: "Unpark or Undo brings a skill back." },
    ],
    command: "skill-studio usage",
  },
];

const TAB_STEPS = new Map([
  ["ArrowRight", 1],
  ["ArrowDown", 1],
  ["ArrowLeft", -1],
  ["ArrowUp", -1],
]);

export function JobsSection() {
  const [active, setActive] = useState<JobId>("update");
  const job = jobs.find((j) => j.id === active)!;

  const onTabKey = (event: KeyboardEvent) => {
    const step = TAB_STEPS.get(event.key);
    if (!step) return;
    event.preventDefault();
    const index = jobs.findIndex((j) => j.id === active);
    const next = jobs[(index + step + jobs.length) % jobs.length].id;
    setActive(next);
    document.getElementById(`job-tab-${next}`)?.focus();
  };

  return (
    <>
      <h2 {...stylex.props(homeSectionStyles.title, styles.title)}>Jobs you do by hand today.</h2>
      <p {...stylex.props(styles.lede)}>
        Each agent keeps its own skill folders. Every update, fix and clean-up means a trip through
        all of them. Skill Studio shows them in one place.
      </p>
      <div {...stylex.props(homeSectionStyles.columns)}>
        <div
          role="tablist"
          aria-label="Skill jobs"
          aria-orientation="vertical"
          onKeyDown={onTabKey}
          {...stylex.props(styles.tabs)}
        >
          {jobs.map(({ id, icon: Icon, label, blurb }) => (
            <button
              key={id}
              type="button"
              role="tab"
              id={`job-tab-${id}`}
              aria-selected={active === id}
              aria-controls="job-panel"
              tabIndex={active === id ? 0 : -1}
              onClick={() => setActive(id)}
              {...stylex.props(styles.tab, active === id && styles.tabOn)}
            >
              <span {...stylex.props(styles.tabIcon, active === id && styles.tabIconOn)}>
                <Icon size={18} aria-hidden="true" />
              </span>
              <span {...stylex.props(styles.tabText)}>
                <span {...stylex.props(styles.tabLabel)}>{label}</span>
                <span {...stylex.props(styles.tabBlurb)}>{blurb}</span>
              </span>
            </button>
          ))}
        </div>

        <div
          id="job-panel"
          role="tabpanel"
          aria-labelledby={`job-tab-${active}`}
          {...stylex.props(styles.panel)}
        >
          <div {...stylex.props(styles.card, styles.byHandCard)}>
            <h3 {...stylex.props(styles.cardTitle)}>
              By hand
              <span {...stylex.props(styles.stepCount)}>{job.byHand.length} steps</span>
            </h3>
            <ol {...stylex.props(styles.steps)}>
              {job.byHand.map((step, index) => (
                <li key={step.text} {...stylex.props(styles.step)}>
                  <span aria-hidden="true" {...stylex.props(styles.stepNumber)}>
                    {index + 1}
                  </span>
                  <span {...stylex.props(styles.stepBody)}>
                    {step.text}
                    {step.code && (
                      <code {...stylex.props(homeSectionStyles.code, styles.stepCode)}>
                        {step.code}
                      </code>
                    )}
                  </span>
                </li>
              ))}
            </ol>
            <p {...stylex.props(styles.byHandCost)}>{job.byHandCost}</p>
          </div>

          <div {...stylex.props(styles.card, styles.withCard)}>
            <h3 {...stylex.props(styles.cardTitle)}>
              With Skill Studio
              <span {...stylex.props(styles.stepCount)}>{job.withStudio.length} steps</span>
            </h3>
            <ul {...stylex.props(styles.steps)}>
              {job.withStudio.map((step) => (
                <li key={step.text} {...stylex.props(styles.step)}>
                  <span aria-hidden="true" {...stylex.props(styles.check)}>
                    <Check size={13} strokeWidth={2.5} />
                  </span>
                  <span {...stylex.props(styles.stepBody)}>{step.text}</span>
                </li>
              ))}
            </ul>
            <p {...stylex.props(styles.commandLine)}>
              <span {...stylex.props(styles.commandLabel)}>From a terminal</span>
              <code {...stylex.props(homeSectionStyles.code, styles.command)}>{job.command}</code>
            </p>
          </div>
        </div>
      </div>
    </>
  );
}

const styles = stylex.create({
  title: { marginBottom: 14 },
  lede: {
    color: siteTokens.muted,
    fontSize: 17,
    lineHeight: 1.6,
    margin: "0 0 40px",
    maxWidth: "58ch",
    textWrap: "pretty",
    "@media (max-width: 760px)": { fontSize: 16, marginBottom: 28 },
  },
  tabs: {
    display: "flex",
    flexDirection: "column",
    gap: 6,
    "@media (max-width: 760px)": {
      display: "grid",
      gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
      marginBottom: 20,
    },
  },
  tab: {
    alignItems: "center",
    backgroundColor: "transparent",
    borderColor: "transparent",
    borderRadius: 12,
    borderStyle: "solid",
    borderWidth: 1,
    color: siteTokens.muted,
    cursor: "pointer",
    display: "flex",
    fontFamily: "inherit",
    gap: 12,
    minHeight: 64,
    padding: "10px 12px",
    textAlign: "left",
    transition:
      "background-color 150ms ease-out, border-color 150ms ease-out, color 150ms ease-out",
    ":hover": { backgroundColor: siteTokens.surface, color: siteTokens.text },
    ":focus-visible": { outline: `2px solid ${siteTokens.text}`, outlineOffset: 2 },
    "@media (max-width: 760px)": { alignItems: "flex-start", gap: 10, minHeight: 56 },
  },
  tabOn: {
    backgroundColor: siteTokens.surface,
    borderColor: siteTokens.border,
    color: siteTokens.text,
  },
  tabIcon: {
    alignItems: "center",
    borderRadius: 9,
    boxShadow: `inset 0 0 0 1px ${siteTokens.border}`,
    display: "flex",
    flexShrink: 0,
    height: 36,
    justifyContent: "center",
    transition: "background-color 150ms ease-out, color 150ms ease-out",
    width: 36,
    "@media (max-width: 760px)": { display: "none" },
  },
  tabIconOn: {
    backgroundColor: siteTokens.accent,
    boxShadow: "none",
    color: siteTokens.accentText,
  },
  tabText: { display: "flex", flexDirection: "column", gap: 2, minWidth: 0 },
  tabLabel: {
    fontSize: 15,
    fontWeight: 640,
    lineHeight: 1.3,
    "@media (max-width: 760px)": { fontSize: 14 },
  },
  tabBlurb: { fontSize: 13, lineHeight: 1.35, "@media (max-width: 760px)": { display: "none" } },
  panel: {
    display: "grid",
    gap: 16,
    gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
    minWidth: 0,
    "@media (max-width: 1000px)": { gridTemplateColumns: "1fr" },
  },
  card: {
    borderRadius: 14,
    borderStyle: "solid",
    borderWidth: 1,
    display: "flex",
    flexDirection: "column",
    gap: 18,
    padding: "22px 22px 20px",
    "@media (max-width: 600px)": { padding: "18px 16px" },
  },
  byHandCard: {
    backgroundColor: "transparent",
    borderColor: siteTokens.border,
    borderStyle: "dashed",
  },
  withCard: {
    backgroundColor: siteTokens.surface,
    borderColor: siteTokens.border,
    boxShadow: `0 0 0 1px ${siteTokens.accentSoft}, 0 18px 50px oklch(0 0 0 / .12)`,
  },
  cardTitle: {
    alignItems: "baseline",
    color: siteTokens.text,
    display: "flex",
    fontSize: 15,
    fontWeight: 650,
    justifyContent: "space-between",
    letterSpacing: "-.01em",
    margin: 0,
  },
  stepCount: { color: siteTokens.muted, fontSize: 12, fontWeight: 500 },
  steps: {
    display: "flex",
    flexDirection: "column",
    gap: 14,
    listStyle: "none",
    margin: 0,
    padding: 0,
  },
  step: { display: "flex", gap: 12 },
  stepNumber: {
    alignItems: "center",
    borderColor: siteTokens.border,
    borderRadius: 999,
    borderStyle: "solid",
    borderWidth: 1,
    color: siteTokens.muted,
    display: "flex",
    flexShrink: 0,
    fontSize: 11,
    fontVariantNumeric: "tabular-nums",
    height: 22,
    justifyContent: "center",
    width: 22,
  },
  check: {
    alignItems: "center",
    backgroundColor: siteTokens.accent,
    borderRadius: 999,
    color: siteTokens.accentText,
    display: "flex",
    flexShrink: 0,
    height: 22,
    justifyContent: "center",
    width: 22,
  },
  stepBody: {
    color: siteTokens.text,
    display: "flex",
    flexDirection: "column",
    fontSize: 14.5,
    gap: 8,
    lineHeight: 1.5,
    minWidth: 0,
    paddingTop: 1,
    textWrap: "pretty",
  },
  stepCode: {
    backgroundColor: siteTokens.surface,
    borderRadius: 7,
    color: siteTokens.muted,
    display: "block",
    fontSize: 12,
    overflowWrap: "anywhere",
    padding: "7px 10px",
  },
  byHandCost: {
    borderTopColor: siteTokens.border,
    borderTopStyle: "dashed",
    borderTopWidth: 1,
    color: siteTokens.muted,
    fontSize: 13,
    lineHeight: 1.5,
    margin: "auto 0 0",
    paddingTop: 14,
    textWrap: "pretty",
  },
  commandLine: {
    borderTopColor: siteTokens.border,
    borderTopStyle: "solid",
    borderTopWidth: 1,
    display: "flex",
    flexDirection: "column",
    gap: 8,
    margin: "auto 0 0",
    minWidth: 0,
    paddingTop: 14,
  },
  commandLabel: { color: siteTokens.muted, fontSize: 12 },
  command: {
    backgroundColor: siteTokens.background,
    borderRadius: 7,
    color: siteTokens.text,
    display: "block",
    fontSize: 12,
    overflowX: "auto",
    padding: "8px 10px",
    whiteSpace: "nowrap",
  },
});
