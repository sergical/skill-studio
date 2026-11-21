// ============================================================================
// Skill Studio - Jobs section
// Five everyday skill jobs, each shown by hand and then with Skill Studio.
// Pack names are fictional; every command and app action is real.
// ============================================================================
import { useState, type KeyboardEvent } from "react";
import * as stylex from "@stylexjs/stylex";
import {
  Check,
  FolderSearch,
  ListTree,
  PackageOpen,
  Stethoscope,
  Users,
  type LucideIcon,
} from "lucide-react";

import { siteTokens } from "../SiteTheme.stylex";
import { homeSectionStyles } from "./HomeSection.stylex";

type JobId = "ignored" | "unused" | "budget" | "update" | "share";

interface Step {
  text: string;
  code?: string;
}

interface Job {
  id: JobId;
  icon: LucideIcon;
  label: string;
  blurb: string;
  /** Only real actions count as steps. Facts about the problem go in `byHandCost`. */
  byHand: Step[];
  /** What multiplies the steps, for example "for each skill". */
  byHandRepeat: string;
  byHandCost: string;
  /** The one action in the app. The finding work is already done when the user arrives. */
  studioStep: string;
  /** What the app already did before the step, and what happens after it. Not steps. */
  studioNotes: string[];
  command: string;
}

const jobs: ReadonlyArray<Job> = [
  {
    id: "ignored",
    icon: Stethoscope,
    label: "Your agent ignores a skill",
    blurb: "See what the model saw.",
    byHand: [
      {
        text: "Find the session file and the skill list in it. Claude Code and Codex record the list. The other agents do not.",
        code: "grep -l skill_listing ~/.claude/projects/*/*.jsonl",
      },
      { text: "Read the list. Check if the skill has a description there, or only a name." },
      {
        text: "Check that the skill is in a folder this agent reads. Claude Code does not read .agents/skills.",
      },
      {
        text: "Open SKILL.md and check the header. A bad header can stop the agent from loading the skill.",
      },
    ],
    byHandRepeat: "for each session",
    byHandCost:
      "Each agent writes its sessions in a different format, and none of them tells you why it skipped a skill.",
    studioStep: "Open Sessions.",
    studioNotes: [
      "Sessions shows what each session listed and used: each skill described, by name only, or not listed. It marks a skill as possibly missed. That mark is a guess from your first prompt.",
      "Then fix the cause. Raise the budget in Model listing, reword the description, or make the skills you only type manual-only. For a broken skill, click Fix.",
    ],
    command: "skill-studio sessions",
  },
  {
    id: "unused",
    icon: FolderSearch,
    label: "Your agents load skills you never use",
    blurb: "Park them. Undo any time.",
    byHand: [
      { text: "List the skills in the agent's folders." },
      {
        text: "Search the agent's session files to find which skills ran. Each agent uses a different format.",
      },
      { text: "Delete the folders you think you do not need, and hope that you are right." },
    ],
    byHandRepeat: "for each agent",
    byHandCost:
      "Each listed skill takes space in the list, used or not. That space is what other skills need to keep a description.",
    studioStep: "Click Park on the skills you do not need.",
    studioNotes: [
      "Sessions already lists each skill that was listed in nearly every session and never used. On one machine, that was 119 skills.",
      "A parked skill leaves your agents' lists. Nothing is deleted, and Unpark brings it back.",
    ],
    command: "skill-studio usage",
  },
  {
    id: "budget",
    icon: ListTree,
    label: "Your skills do not fit the list",
    blurb: "Try another budget first.",
    byHand: [
      {
        text: "Find the listing budget of each agent. Claude Code and Codex each take a share of the context window, and each has its own setting.",
      },
      {
        text: "Add up the length of each name and description. Compare the total with the budget.",
      },
      {
        text: "Change the setting, start a new session, and ask the agent which skills it sees.",
      },
    ],
    byHandRepeat: "for each agent",
    byHandCost: "No agent tells you which descriptions it cut, or which it would cut next.",
    studioStep: "Type a budget in Model listing.",
    studioNotes: [
      "Model listing already shows each skill as described, name only or hidden. Compare marks each skill that gains or loses a description at the new budget.",
      "Skill Studio does not write agent settings. It gives you the line and the file to put it in.",
    ],
    command: "skill-studio listing --claude-fraction 0.02",
  },
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
      { text: "Copy the new files into each agent's folder." },
    ],
    byHandRepeat: "for each skill",
    byHandCost: "One folder per agent. Miss one and that agent keeps the old version.",
    studioStep: "Click Update all on Home.",
    studioNotes: [
      "Home already lists each skill that has a new version. With only one, its row has Update. If you edited a skill, Home asks before it replaces your edits.",
      "Each agent that links the skill gets the new version.",
    ],
    command: "skill-studio outdated",
  },
  {
    id: "share",
    icon: Users,
    label: "A teammate asks for your setup",
    blurb: "Share one repo, not a list.",
    byHand: [
      {
        text: "Read the lock file to find the repo of each skill.",
        code: "cat ~/.agents/.skill-lock.json",
      },
      { text: "Look in each agent's folder for skills that are not in the lock file." },
      { text: "Guess which skills you wrote and which ones you changed." },
      { text: "Paste a list of repos into chat. Zip the rest." },
    ],
    byHandRepeat: "each time someone asks",
    byHandCost: "The lock file knows only one installer. Your own skills are not in it.",
    studioStep: "Your teammate adds your repo in Packs.",
    studioNotes: [
      "Your skills live in one git repo, such as acme-labs/review-skills. Packs lists the skills it will add, and one more click adds them.",
      "Check upstream finds new versions and new skills. Update skips each skill your teammate edited, and says why. Each machine needs Node.js and git.",
    ],
    command: "skill-studio pack add acme-labs/review-skills",
  },
];

// On a phone the tabs sit in two columns. With an odd count the last tab takes the full row.
const LAST_TAB_ALONE = jobs.length % 2 === 1;

const TAB_STEPS = new Map([
  ["ArrowRight", 1],
  ["ArrowDown", 1],
  ["ArrowLeft", -1],
  ["ArrowUp", -1],
]);

export function JobsSection() {
  const [active, setActive] = useState<JobId>("ignored");
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
        Each agent keeps its own skill folders and its own session files. To learn what a model saw,
        you open them by hand. Skill Studio reads them for you.
      </p>
      <div {...stylex.props(homeSectionStyles.columns)}>
        <div
          role="tablist"
          aria-label="Skill jobs"
          aria-orientation="vertical"
          onKeyDown={onTabKey}
          {...stylex.props(styles.tabs)}
        >
          {jobs.map(({ id, icon: Icon, label, blurb }, index) => (
            <button
              key={id}
              type="button"
              role="tab"
              id={`job-tab-${id}`}
              aria-selected={active === id}
              aria-controls="job-panel"
              tabIndex={active === id ? 0 : -1}
              onClick={() => setActive(id)}
              {...stylex.props(
                styles.tab,
                active === id && styles.tabOn,
                LAST_TAB_ALONE && index === jobs.length - 1 && styles.tabWide,
              )}
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
              <span {...stylex.props(styles.stepCount)}>
                {job.byHand.length} steps, {job.byHandRepeat}
              </span>
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
              <span {...stylex.props(styles.stepCount, styles.oneStep)}>1 step</span>
            </h3>
            <p {...stylex.props(styles.studioStep)}>
              <span aria-hidden="true" {...stylex.props(styles.check)}>
                <Check size={16} strokeWidth={2.5} />
              </span>
              {job.studioStep}
            </p>
            <ul {...stylex.props(styles.notes)}>
              {job.studioNotes.map((note) => (
                <li key={note}>{note}</li>
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
  tabWide: { "@media (max-width: 760px)": { gridColumn: "1 / -1" } },
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
    "@media (max-width: 1000px)": { gridTemplateColumns: "minmax(0, 1fr)" },
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
  oneStep: { color: siteTokens.text, fontWeight: 650 },
  studioStep: {
    alignItems: "flex-start",
    color: siteTokens.text,
    display: "flex",
    fontSize: 22,
    fontWeight: 650,
    gap: 12,
    letterSpacing: "-.015em",
    lineHeight: 1.3,
    margin: 0,
    textWrap: "balance",
    "@media (max-width: 600px)": { fontSize: 19 },
  },
  notes: {
    color: siteTokens.muted,
    display: "flex",
    flexDirection: "column",
    fontSize: 14,
    gap: 10,
    lineHeight: 1.5,
    listStyle: "none",
    margin: 0,
    padding: "0 0 0 40px",
    textWrap: "pretty",
    "@media (max-width: 600px)": { paddingLeft: 0 },
  },
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
    height: 28,
    justifyContent: "center",
    marginTop: -1,
    width: 28,
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
