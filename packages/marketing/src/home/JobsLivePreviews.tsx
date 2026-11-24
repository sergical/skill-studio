// ============================================================================
// Skill Studio - Interactive live previews for JobsSection
// Each resolved scenario renders a working miniature of the corresponding
// Skill Studio screen (Fix/Undo, Park/Unpark, 1%/2% budget diff, Sync, Packs).
// ============================================================================
import { useState } from "react";
import * as stylex from "@stylexjs/stylex";
import { Check, Copy, GitBranch, RefreshCw, RotateCcw, Wrench } from "lucide-react";

import { AgentIcon } from "../AgentIcon";
import { siteTokens } from "../SiteTheme.stylex";
import { homeSectionStyles } from "./HomeSection.stylex";
import type { Scenario } from "./jobs-scenarios";
import { useCopyFlash } from "./useCopyFlash";

export function CopyCliButton({ command }: { command: string }) {
  const { copied, copy } = useCopyFlash();
  return (
    <button
      type="button"
      onClick={() => copy(command)}
      aria-label={copied ? `Copied ${command}` : `Copy ${command}`}
      {...stylex.props(styles.cmdButton, copied && styles.cmdButtonCopied)}
    >
      <code {...stylex.props(homeSectionStyles.code, styles.cmdText)}>{command}</code>
      <span aria-hidden="true" {...stylex.props(styles.cmdIcon, copied && styles.cmdIconCopied)}>
        {copied ? <Check size={13} /> : <Copy size={13} />}
      </span>
    </button>
  );
}

function SkippedPreview({ initialFixed = false }: { initialFixed?: boolean }) {
  const [fixed, setFixed] = useState(initialFixed);
  return (
    <div {...stylex.props(styles.frame)}>
      <div {...stylex.props(styles.head)}>
        <span {...stylex.props(styles.headTitle)}>
          <AgentIcon agent="claude" size={14} />
          Sessions · acme-web
        </span>
        <span {...stylex.props(styles.headMeta, styles.num)}>
          {fixed ? "215 listed · 0 errors" : "215 listed · 2 issues"}
        </span>
      </div>
      <ul {...stylex.props(styles.list)}>
        <li {...stylex.props(styles.row)}>
          <span {...stylex.props(styles.dot, styles.dotSuccess)} />
          <span {...stylex.props(styles.name)}>commit-conventional</span>
          <span {...stylex.props(styles.tagMuted)}>Described · ran 4×</span>
        </li>
        <li {...stylex.props(styles.row)}>
          <span {...stylex.props(styles.dot, styles.dotWarn)} />
          <span {...stylex.props(styles.name)}>writing-skills-guide</span>
          <span {...stylex.props(styles.tagWarn)}>Name only · missed 1</span>
        </li>
        <li {...stylex.props(styles.row, styles.rowLast)}>
          <span {...stylex.props(styles.dot, fixed ? styles.dotSuccess : styles.dotError)} />
          <span {...stylex.props(styles.name)}>pdf-tools-extractor</span>
          {fixed ? (
            <span {...stylex.props(styles.endGroup)}>
              <span {...stylex.props(styles.tagSuccess)}>Link repaired</span>
              <button
                type="button"
                onClick={() => setFixed(false)}
                {...stylex.props(styles.btnGhost)}
              >
                <RotateCcw size={11} aria-hidden="true" />
                Undo
              </button>
            </span>
          ) : (
            <span {...stylex.props(styles.endGroup)}>
              <span {...stylex.props(styles.tagError)}>Broken link</span>
              <button
                type="button"
                onClick={() => setFixed(true)}
                {...stylex.props(styles.btnSolid)}
              >
                <Wrench size={11} aria-hidden="true" />
                Fix
              </button>
            </span>
          )}
        </li>
      </ul>
    </div>
  );
}

const IDLE_LIST = [
  { id: "release-notes-generator", meta: "98 sessions · 0 runs" },
  { id: "deploy-preview-cloudflare", meta: "94 sessions · 0 runs" },
  { id: "legacy-graphql-codegen", meta: "86 sessions · 0 runs" },
] as const;

function UnusedPreview({ startAllParked = false }: { startAllParked?: boolean }) {
  const [parked, setParked] = useState<ReadonlyArray<string>>(
    startAllParked ? IDLE_LIST.map((s) => s.id) : ["release-notes-generator"],
  );
  const toggle = (id: string) =>
    setParked((prev) => (prev.includes(id) ? prev.filter((item) => item !== id) : [...prev, id]));
  return (
    <div {...stylex.props(styles.frame)}>
      <div {...stylex.props(styles.head)}>
        <span {...stylex.props(styles.headTitle)}>Idle in last 30 days</span>
        <span
          {...stylex.props(styles.headMeta, styles.num, parked.length > 0 && styles.textSuccess)}
        >
          {parked.length} parked · +{parked.length} slots freed
        </span>
      </div>
      <ul {...stylex.props(styles.list)}>
        {IDLE_LIST.map((skill, idx) => {
          const isParked = parked.includes(skill.id);
          return (
            <li
              key={skill.id}
              {...stylex.props(
                styles.row,
                idx === IDLE_LIST.length - 1 && styles.rowLast,
                isParked && styles.rowDim,
              )}
            >
              <span {...stylex.props(styles.dot, isParked ? styles.dotMuted : styles.dotWarn)} />
              <div {...stylex.props(styles.stack)}>
                <span {...stylex.props(styles.name)}>{skill.id}</span>
                <span {...stylex.props(styles.sub)}>{skill.meta}</span>
              </div>
              <button
                type="button"
                onClick={() => toggle(skill.id)}
                {...stylex.props(isParked ? styles.btnGhost : styles.btnSolid)}
              >
                {isParked ? "Unpark" : "Park"}
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

function BudgetPreview() {
  const [fraction, setFraction] = useState<1 | 2>(2);
  const described = fraction === 1 ? 145 : 215;
  const nameOnly = fraction === 1 ? 70 : 0;
  return (
    <div {...stylex.props(styles.frame)}>
      <div {...stylex.props(styles.head)}>
        <span {...stylex.props(styles.headTitle)}>
          <AgentIcon agent="claude" size={14} />
          Model listing compare
        </span>
        <div role="group" aria-label="Budget comparison" {...stylex.props(styles.seg)}>
          {([1, 2] as const).map((pct) => (
            <button
              key={pct}
              type="button"
              aria-pressed={fraction === pct}
              onClick={() => setFraction(pct)}
              {...stylex.props(styles.segBtn, fraction === pct && styles.segBtnOn)}
            >
              {pct}%
            </button>
          ))}
        </div>
      </div>
      <div {...stylex.props(styles.stats)}>
        <div {...stylex.props(styles.stat)}>
          <span {...stylex.props(styles.statNum, styles.num)}>{described}</span>
          <span {...stylex.props(styles.statLabel)}>Described</span>
        </div>
        <div {...stylex.props(styles.stat)}>
          <span
            {...stylex.props(
              styles.statNum,
              styles.num,
              nameOnly === 0 ? styles.textSuccess : styles.textWarn,
            )}
          >
            {nameOnly}
          </span>
          <span {...stylex.props(styles.statLabel)}>Name only</span>
        </div>
        <div {...stylex.props(styles.stat)}>
          <span {...stylex.props(styles.statNum, styles.num, fraction === 2 && styles.textSuccess)}>
            {fraction === 2 ? "+70" : "0"}
          </span>
          <span {...stylex.props(styles.statLabel)}>Regained</span>
        </div>
      </div>
      <div {...stylex.props(styles.settingBox)}>
        <span {...stylex.props(styles.sub)}>~/.claude/settings.json</span>
        <code {...stylex.props(homeSectionStyles.code, styles.settingLine)}>
          {`"skillListingBudgetFraction": ${fraction === 1 ? "0.01" : "0.02"}`}
        </code>
      </div>
    </div>
  );
}

function UpdatePreview({ initialUpdated = false }: { initialUpdated?: boolean }) {
  const [updated, setUpdated] = useState(initialUpdated);
  return (
    <div {...stylex.props(styles.frame)}>
      <div {...stylex.props(styles.head)}>
        <span {...stylex.props(styles.headTitle)}>Outdated across 4 agents</span>
        <button
          type="button"
          onClick={() => setUpdated((v) => !v)}
          {...stylex.props(updated ? styles.btnGhost : styles.btnSolid)}
        >
          <RefreshCw size={11} aria-hidden="true" />
          {updated ? "Reset" : "Update safe"}
        </button>
      </div>
      <ul {...stylex.props(styles.list)}>
        <li {...stylex.props(styles.row)}>
          <span {...stylex.props(styles.dot, updated ? styles.dotSuccess : styles.dotWarn)} />
          <div {...stylex.props(styles.stack)}>
            <span {...stylex.props(styles.name)}>react-best-practices</span>
            <span {...stylex.props(styles.sub)}>Claude · Codex · Cursor · OpenCode</span>
          </div>
          <span {...stylex.props(updated ? styles.tagSuccess : styles.tagMuted, styles.num)}>
            {updated ? "v1.7 synced" : "v1.4 → v1.7"}
          </span>
        </li>
        <li {...stylex.props(styles.row, styles.rowLast)}>
          <span {...stylex.props(styles.dot, styles.dotMuted)} />
          <div {...stylex.props(styles.stack)}>
            <span {...stylex.props(styles.name)}>deploy-preview-cloudflare</span>
            <span {...stylex.props(styles.sub)}>Edited locally 2 days ago</span>
          </div>
          <span {...stylex.props(styles.tagWarn)}>Protected</span>
        </li>
      </ul>
    </div>
  );
}

function SharePreview() {
  const skills = ["commit-conventional", "pr-review-checklist", "design-tokens-audit"] as const;
  const [selected, setSelected] = useState<ReadonlyArray<string>>(skills);
  const toggle = (name: string) =>
    setSelected((prev) =>
      prev.includes(name) ? prev.filter((item) => item !== name) : [...prev, name],
    );
  return (
    <div {...stylex.props(styles.frame)}>
      <div {...stylex.props(styles.head)}>
        <span {...stylex.props(styles.headTitle)}>
          <GitBranch size={13} aria-hidden="true" />
          acme/skills · main
        </span>
        <span {...stylex.props(styles.headMeta, styles.num)}>
          {selected.length} of {skills.length} selected
        </span>
      </div>
      <ul {...stylex.props(styles.list)}>
        {skills.map((skill, idx) => {
          const on = selected.includes(skill);
          return (
            <li
              key={skill}
              {...stylex.props(styles.row, idx === skills.length - 1 && styles.rowLast)}
            >
              <button
                type="button"
                role="checkbox"
                aria-checked={on}
                onClick={() => toggle(skill)}
                {...stylex.props(styles.checkRow)}
              >
                <span {...stylex.props(styles.checkbox, on && styles.checkboxOn)}>
                  {on && <Check size={10} strokeWidth={3} aria-hidden="true" />}
                </span>
                <span {...stylex.props(styles.name)}>{skill}</span>
                <span {...stylex.props(styles.tagMuted)}>All agents</span>
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

export function LiveScenarioPreview({
  id,
  resolved = false,
}: {
  id: Scenario["id"];
  resolved?: boolean;
}) {
  switch (id) {
    case "ignored":
      return <SkippedPreview initialFixed={resolved} />;
    case "unused":
      return <UnusedPreview startAllParked={resolved} />;
    case "budget":
      return <BudgetPreview />;
    case "update":
      return <UpdatePreview initialUpdated={resolved} />;
    case "share":
      return <SharePreview />;
  }
}

const HOVER = "@media (hover: hover) and (pointer: fine)";
const PRESS_EASE = "cubic-bezier(0.25, 0.46, 0.45, 0.94)";

const styles = stylex.create({
  frame: {
    backgroundColor: siteTokens.background,
    borderColor: siteTokens.border,
    borderRadius: 12,
    borderStyle: "solid",
    borderWidth: 1,
    overflow: "hidden",
  },
  head: {
    alignItems: "center",
    borderBottomColor: siteTokens.border,
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    display: "flex",
    flexWrap: "wrap",
    gap: 8,
    justifyContent: "space-between",
    minHeight: 42,
    padding: "6px 12px",
  },
  headTitle: {
    alignItems: "center",
    color: siteTokens.text,
    display: "inline-flex",
    fontSize: 12.5,
    fontWeight: 620,
    gap: 7,
  },
  headMeta: { color: siteTokens.muted, fontSize: 11.5 },
  num: { fontVariantNumeric: "tabular-nums" },
  list: { listStyle: "none", margin: 0, padding: 0 },
  row: {
    alignItems: "center",
    borderBottomColor: siteTokens.border,
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    display: "flex",
    gap: 9,
    minHeight: 46,
    padding: "6px 12px",
    transition: "opacity 160ms ease-out",
  },
  rowLast: { borderBottomWidth: 0 },
  rowDim: { opacity: 0.5 },
  stack: { display: "flex", flex: 1, flexDirection: "column", gap: 1, minWidth: 0 },
  name: {
    color: siteTokens.text,
    flex: 1,
    fontSize: 13,
    fontWeight: 580,
    minWidth: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  sub: {
    color: siteTokens.muted,
    fontSize: 11.5,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  endGroup: { alignItems: "center", display: "inline-flex", flexShrink: 0, gap: 7 },
  dot: { borderRadius: 999, flexShrink: 0, height: 7, width: 7 },
  dotSuccess: { backgroundColor: siteTokens.success },
  dotWarn: { backgroundColor: siteTokens.warning },
  dotError: { backgroundColor: "oklch(0.68 0.19 25)" },
  dotMuted: { backgroundColor: siteTokens.muted },
  tagMuted: { color: siteTokens.muted, flexShrink: 0, fontSize: 11.5 },
  tagWarn: { color: siteTokens.warning, flexShrink: 0, fontSize: 11.5, fontWeight: 560 },
  tagError: { color: "oklch(0.72 0.18 25)", flexShrink: 0, fontSize: 11.5, fontWeight: 560 },
  tagSuccess: { color: siteTokens.success, flexShrink: 0, fontSize: 11.5, fontWeight: 580 },
  textSuccess: { color: siteTokens.success },
  textWarn: { color: siteTokens.warning },
  btnSolid: {
    alignItems: "center",
    backgroundColor: { default: siteTokens.text, [HOVER]: { ":hover": siteTokens.accent } },
    borderRadius: 6,
    borderWidth: 0,
    color: siteTokens.background,
    cursor: "pointer",
    display: "inline-flex",
    flexShrink: 0,
    fontFamily: "inherit",
    fontSize: 11.5,
    fontWeight: 620,
    gap: 5,
    minHeight: 28,
    paddingInline: 9,
    position: "relative",
    transition: `background-color 150ms ease, transform 150ms ${PRESS_EASE}`,
    "::before": { content: '""', inset: "-8px -2px", position: "absolute" },
    ":active": { transform: "scale(.97)" },
    ":focus-visible": { outline: `2px solid ${siteTokens.muted}`, outlineOffset: 2 },
  },
  btnGhost: {
    alignItems: "center",
    backgroundColor: "transparent",
    borderColor: { default: siteTokens.border, [HOVER]: { ":hover": siteTokens.muted } },
    borderRadius: 6,
    borderStyle: "solid",
    borderWidth: 1,
    color: siteTokens.text,
    cursor: "pointer",
    display: "inline-flex",
    flexShrink: 0,
    fontFamily: "inherit",
    fontSize: 11.5,
    fontWeight: 580,
    gap: 5,
    minHeight: 28,
    paddingInline: 9,
    position: "relative",
    transition: `border-color 150ms ease, transform 150ms ${PRESS_EASE}`,
    "::before": { content: '""', inset: "-8px -2px", position: "absolute" },
    ":active": { transform: "scale(.97)" },
    ":focus-visible": { outline: `2px solid ${siteTokens.muted}`, outlineOffset: 2 },
  },
  seg: {
    backgroundColor: siteTokens.surface,
    borderColor: siteTokens.border,
    borderRadius: 7,
    borderStyle: "solid",
    borderWidth: 1,
    display: "inline-flex",
    gap: 2,
    padding: 2,
  },
  segBtn: {
    backgroundColor: "transparent",
    borderRadius: 5,
    borderWidth: 0,
    color: siteTokens.muted,
    cursor: "pointer",
    fontFamily: "inherit",
    fontSize: 11.5,
    fontWeight: 600,
    minHeight: 24,
    minWidth: 36,
    paddingInline: 7,
    transition: "background-color 150ms ease-out, color 150ms ease-out",
    ":focus-visible": { outline: `2px solid ${siteTokens.muted}`, outlineOffset: 1 },
  },
  segBtnOn: { backgroundColor: siteTokens.text, color: siteTokens.background },
  stats: {
    borderBottomColor: siteTokens.border,
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    display: "grid",
    gap: 10,
    gridTemplateColumns: "repeat(3, minmax(0, 1fr))",
    padding: "14px 12px",
  },
  stat: { display: "flex", flexDirection: "column", gap: 2 },
  statNum: {
    color: siteTokens.text,
    fontSize: 24,
    fontWeight: 700,
    letterSpacing: "-.03em",
    lineHeight: 1,
  },
  statLabel: { color: siteTokens.muted, fontSize: 11.5 },
  settingBox: {
    display: "flex",
    flexDirection: "column",
    gap: 3,
    padding: "10px 12px",
  },
  settingLine: { color: siteTokens.text, fontSize: 12, overflowWrap: "anywhere" },
  checkRow: {
    alignItems: "center",
    backgroundColor: "transparent",
    borderWidth: 0,
    color: "inherit",
    cursor: "pointer",
    display: "flex",
    fontFamily: "inherit",
    gap: 9,
    margin: "-6px -12px",
    minHeight: 46,
    padding: "6px 12px",
    textAlign: "left",
    width: "calc(100% + 24px)",
    ":focus-visible": { outline: `2px solid ${siteTokens.muted}`, outlineOffset: -2 },
  },
  checkbox: {
    alignItems: "center",
    borderColor: siteTokens.border,
    borderRadius: 4,
    borderStyle: "solid",
    borderWidth: 1,
    color: siteTokens.background,
    display: "inline-flex",
    flexShrink: 0,
    height: 15,
    justifyContent: "center",
    width: 15,
  },
  checkboxOn: { backgroundColor: siteTokens.text, borderColor: siteTokens.text },
  cmdButton: {
    alignItems: "center",
    backgroundColor: siteTokens.background,
    borderColor: { default: siteTokens.border, [HOVER]: { ":hover": siteTokens.muted } },
    borderRadius: 9,
    borderStyle: "solid",
    borderWidth: 1,
    color: siteTokens.text,
    cursor: "pointer",
    display: "inline-flex",
    fontFamily: "inherit",
    gap: 8,
    maxWidth: "100%",
    minHeight: 36,
    minWidth: 0,
    padding: "0 10px 0 11px",
    position: "relative",
    transition: `border-color 150ms ease, color 150ms ease, transform 150ms ${PRESS_EASE}`,
    "::before": { content: '""', inset: "-4px 0", position: "absolute" },
    ":active": { transform: "scale(.97)" },
    ":focus-visible": { outline: `2px solid ${siteTokens.muted}`, outlineOffset: 2 },
  },
  cmdButtonCopied: {
    borderColor: { default: siteTokens.success, [HOVER]: { ":hover": siteTokens.success } },
    color: siteTokens.success,
  },
  cmdText: {
    fontSize: 12,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  cmdIcon: { color: siteTokens.muted, display: "inline-flex", flexShrink: 0 },
  cmdIconCopied: { color: siteTokens.success },
});
