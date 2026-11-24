import type { ReactNode } from "react";
import * as stylex from "@stylexjs/stylex";
import { ChevronDown } from "lucide-react";

import { siteTokens } from "../SiteTheme.stylex";
import { DOCS_URL } from "../site-links";
import { homeSectionStyles as section } from "./HomeSection.stylex";

function Code({ children }: { children: ReactNode }) {
  return <code {...stylex.props(section.code)}>{children}</code>;
}

const questions: ReadonlyArray<{ question: string; answer: ReactNode }> = [
  {
    question: "Which agents does it work with?",
    answer: (
      <>
        Claude Code, Codex, OpenCode, pi, Cursor and Grok Build, plus the shared{" "}
        <Code>.agents</Code> folder. It finds skills globally and in the projects you work in.
      </>
    ),
  },
  {
    question: "Which agents record the skill list?",
    answer:
      "Claude Code and Codex record the skill list of each session. The other agents record only the skills they used. For them, Activity shows the skills they used, and Sessions shows the current listing prediction.",
  },
  {
    question: "Does it change my agent settings?",
    answer:
      "No. Skill Studio reads agent settings and does not write them. When a setting must change, such as the listing budget, it gives you the line and the file to put it in.",
  },
  {
    question: "How accurate is the listing?",
    answer:
      "In five Claude Code sessions, 418 of 420 predicted skill states matched the list that the session recorded (99.5%). In 15 Codex sessions, 99.8% of the description lengths matched. Model listing is a prediction. Sessions shows what each session recorded. A possible miss in Sessions is a guess, and the app marks it as one.",
  },
  {
    question: "Does parking delete a skill?",
    answer: "No. The skill moves to a parked folder, and you can unpark it at any time.",
  },
  {
    question: "Can I undo a change?",
    answer:
      "Yes. Changes go into a history you can undo. Skill Studio keeps a backup of each skill it removes. To undo a park, unpark the skill.",
  },
  {
    question: "Why not just use npx skills?",
    answer: (
      <>
        Skill Studio uses <Code>npx skills</Code> and dotagents to install, update and remove
        skills. It adds what they do not do: what each model sees of your skills, one view of every
        agent's folders, the copies that differ, broken skills, which skills ran, and park with
        undo.
      </>
    ),
  },
  {
    question: "Can I turn a skill off for one agent only?",
    answer:
      "Yes. Park turns a skill off for every agent. To turn it off for one agent only, Skill Studio first splits a shared skill into one copy for each agent, then parks the copy of that agent. It shows what changes before it does anything, and you can undo it.",
  },
  {
    question: "Does it send my data anywhere?",
    answer:
      "No account is needed and it runs on your Mac. It goes online only to browse and install skills, check for updates and send crash reports. Crash reports are anonymous and you can turn them off. They never include skill names, files or paths.",
  },
  {
    question: "Is it free?",
    answer: "Yes. It is open source under the MIT license.",
  },
];

export function FaqSection() {
  return (
    <div {...stylex.props(section.columns)}>
      <div>
        <h2 {...stylex.props(section.title, styles.title)}>Questions.</h2>
        <div {...stylex.props(styles.links)}>
          <a href="#download" {...stylex.props(section.textLink)}>
            Get the Mac app <span aria-hidden="true">→</span>
          </a>
          <a href={DOCS_URL} {...stylex.props(section.textLink, styles.secondaryLink)}>
            Read the docs <span aria-hidden="true">→</span>
          </a>
        </div>
      </div>
      <div {...stylex.props(styles.list)}>
        {questions.map(({ question, answer }) => (
          <details key={question} {...stylex.props(stylex.defaultMarker(), styles.item)}>
            <summary {...stylex.props(styles.summary)}>
              {question}
              <ChevronDown aria-hidden="true" size={14} {...stylex.props(styles.chevron)} />
            </summary>
            <p {...stylex.props(styles.answer)}>{answer}</p>
          </details>
        ))}
      </div>
    </div>
  );
}

const HOVER = "@media (hover: hover) and (pointer: fine)";
const REDUCED = "@media (prefers-reduced-motion: reduce)";

const answerReveal = stylex.keyframes({
  from: { opacity: 0, transform: "translateY(-4px)" },
  to: { opacity: 1, transform: "translateY(0)" },
});

const styles = stylex.create({
  title: {
    marginBottom: 12,
    "@media (max-width: 760px)": { marginBottom: 8 },
  },
  links: {
    alignItems: "flex-start",
    display: "flex",
    flexDirection: "column",
    "@media (max-width: 760px)": { flexDirection: "row", flexWrap: "wrap", gap: "0 20px" },
  },
  secondaryLink: { color: siteTokens.muted },
  list: {
    borderTopColor: siteTokens.border,
    borderTopStyle: "solid",
    borderTopWidth: 1,
    "@media (max-width: 760px)": { marginTop: 20 },
  },
  item: {
    borderBottomColor: siteTokens.border,
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
  },
  summary: {
    alignItems: "center",
    color: siteTokens.text,
    cursor: "pointer",
    display: "flex",
    fontSize: 16,
    fontWeight: 550,
    gap: 12,
    justifyContent: "space-between",
    letterSpacing: "-.02em",
    listStyle: "none",
    minHeight: 58,
    padding: "14px 0",
    ":focus-visible": {
      outlineColor: siteTokens.muted,
      outlineOffset: 4,
      outlineStyle: "solid",
      outlineWidth: 2,
    },
  },
  chevron: {
    color: {
      default: siteTokens.muted,
      [HOVER]: { [stylex.when.ancestor(":hover")]: siteTokens.text },
    },
    flexShrink: 0,
    transform: {
      default: "none",
      [stylex.when.ancestor(":is([open])")]: "rotate(180deg)",
    },
    transition: {
      default: "transform 200ms cubic-bezier(0.25, 1, 0.5, 1), color 150ms ease",
      [REDUCED]: "color 150ms ease",
    },
  },
  answer: {
    animationDuration: "180ms",
    animationName: { default: answerReveal, [REDUCED]: "none" },
    animationTimingFunction: "cubic-bezier(0.25, 1, 0.5, 1)",
    color: siteTokens.muted,
    fontSize: 15,
    lineHeight: 1.6,
    margin: "0 0 22px",
    maxWidth: "62ch",
    textWrap: "pretty",
  },
});
