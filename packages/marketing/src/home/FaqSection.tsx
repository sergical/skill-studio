import * as stylex from "@stylexjs/stylex";
import { ChevronDown } from "lucide-react";

import { siteTokens } from "../SiteTheme.stylex";
import { DOCS_URL } from "../site-links";
import { homeSectionStyles as section } from "./HomeSection.stylex";

export function FaqSection() {
  return (
    <div {...stylex.props(section.columns)}>
      <div>
        <h2 {...stylex.props(section.title, styles.title)}>Questions</h2>
        <a href={DOCS_URL} {...stylex.props(section.textLink)}>
          Read the docs
          <span aria-hidden="true">→</span>
        </a>
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

const styles = stylex.create({
  title: {
    marginBottom: 12,
    "@media (max-width: 760px)": { marginBottom: 8 },
  },
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
    color: siteTokens.muted,
    flexShrink: 0,
    transform: {
      default: "none",
      [stylex.when.ancestor(":is([open])")]: "rotate(180deg)",
    },
    transition: {
      default: "transform 200ms cubic-bezier(0.25, 1, 0.5, 1)",
      "@media (prefers-reduced-motion: reduce)": "none",
    },
  },
  answerLink: { color: siteTokens.text, textDecorationLine: "underline", textUnderlineOffset: 3 },
  answer: {
    color: siteTokens.muted,
    fontSize: 15,
    lineHeight: 1.6,
    margin: "0 0 22px",
    maxWidth: "62ch",
    textWrap: "pretty",
  },
});

const questions = [
  {
    question: "Which agents does it work with?",
    answer:
      "Claude Code, Codex, Cursor, OpenCode, pi, and Grok Build. Claude Code and Codex record the skill list for each session. For the other agents, Skill Studio shows an estimate of the current list.",
  },
  {
    question: "Does it work on Intel Macs?",
    answer: (
      <>
        Yes. The desktop app runs on Apple silicon and Intel Macs with macOS 13 or later.{" "}
        <a href="/docs/#install" {...stylex.props(styles.answerLink)}>
          Choose the Intel download
        </a>
        .
      </>
    ),
  },
  {
    question: "When does it go online?",
    answer:
      "The Skill Store, installs, updates, update checks, and public install-count lookups use the network. The app also sends crash and performance reports when Telemetry is on. You can scan skills without an account.",
  },
] as const;
