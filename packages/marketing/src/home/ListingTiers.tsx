// ============================================================================
// Skill Studio - Listing tiers
// The three states a skill can have in the list an agent gives its model, as
// one bar drawn to scale. The counts are a measurement, not sample data: one
// machine, Claude Code, a 1% listing budget (285 skills). The accuracy figure
// is in docs/agent-skill-conventions.md, the study in docs/research/skill-evals.md.
// ============================================================================
import * as stylex from "@stylexjs/stylex";

import { siteTokens } from "../SiteTheme.stylex";
import { homeSectionStyles } from "./HomeSection.stylex";
import { STATE_COLOR, type PileState } from "./skill-pile-data";

const STUDY_URL = "https://arxiv.org/abs/2610.18522";

const TIERS = [
  {
    state: "described",
    count: 49,
    label: "Described",
    meaning:
      "The model sees the name and the description. It can pick the skill from what you ask.",
  },
  {
    state: "name-only",
    count: 107,
    label: "Name only",
    meaning: "The model sees only the name and must guess from it. The skill can still run.",
  },
  {
    state: "hidden",
    count: 129,
    label: "Hidden",
    meaning:
      "The model does not see the skill. It is manual-only, turned off, parked, or installed for another project.",
  },
] satisfies ReadonlyArray<{ state: PileState; count: number; label: string; meaning: string }>;

const TOTAL = TIERS.reduce((sum, tier) => sum + tier.count, 0);

export function ListingTiers() {
  return (
    <>
      <h2 {...stylex.props(homeSectionStyles.title, styles.title)}>
        Your agent does not read every skill.
      </h2>
      <div {...stylex.props(homeSectionStyles.columns)}>
        <div {...stylex.props(styles.intro)}>
          <p {...stylex.props(styles.lead)}>
            Each agent gives its model a list of skills. Claude Code and Codex limit the size of
            the list. When your skills do not fit, descriptions are shortened or removed first.
          </p>
          <p {...stylex.props(styles.note)}>
            A long list also hurts. In the study{" "}
            <a href={STUDY_URL} target="_blank" rel="noreferrer" {...stylex.props(styles.link)}>
              Lost in Implicit Routing
            </a>
            , agents found the right skill in 73.6% of runs. With 1,000 unrelated skills installed,
            they found it in 27.4%.
          </p>
        </div>
        <figure {...stylex.props(styles.figure)}>
          <figcaption {...stylex.props(styles.caption)}>
            One machine, measured: {TOTAL} skills, Claude Code, a 1% listing budget.
          </figcaption>
          {/* The counts are read again in the list below, so the bar is for the eye only. */}
          <div aria-hidden="true" {...stylex.props(styles.bar)}>
            {TIERS.map((tier) => (
              <span
                key={tier.state}
                {...stylex.props(styles.segment, styles.segmentSize(tier.count))}
              >
                <span {...stylex.props(styles.count)}>{tier.count}</span>
                <span
                  {...stylex.props(
                    styles.fill,
                    styles.fillColor(STATE_COLOR[tier.state]),
                    tier.state === "hidden" && styles.fillHidden,
                  )}
                />
              </span>
            ))}
          </div>
          <dl {...stylex.props(styles.tiers)}>
            {TIERS.map((tier) => (
              <div key={tier.state} {...stylex.props(styles.tier)}>
                <dt {...stylex.props(styles.term)}>
                  <span
                    aria-hidden="true"
                    {...stylex.props(
                      styles.dot,
                      styles.fillColor(STATE_COLOR[tier.state]),
                      tier.state === "hidden" && styles.fillHidden,
                    )}
                  />
                  {tier.label}
                  <span {...stylex.props(styles.visuallyHidden)}>: {tier.count} skills</span>
                </dt>
                <dd {...stylex.props(styles.meaning)}>{tier.meaning}</dd>
              </div>
            ))}
          </dl>
          <p {...stylex.props(styles.proof)}>
            <span {...stylex.props(styles.proofText)}>
              Skill Studio predicts the list before a session starts. In five Claude Code sessions,
              it had 418 of 420 skill states right (99.5%).
            </span>
          </p>
        </figure>
      </div>
    </>
  );
}

const STACKED = "@media (max-width: 760px)";

const styles = stylex.create({
  title: { marginBottom: 36 },
  intro: {
    display: "flex",
    flexDirection: "column",
    gap: 16,
    [STACKED]: { marginBottom: 28 },
  },
  lead: {
    color: siteTokens.text,
    fontSize: 17,
    lineHeight: 1.55,
    margin: 0,
    textWrap: "pretty",
  },
  note: {
    color: siteTokens.muted,
    fontSize: 14,
    lineHeight: 1.55,
    margin: 0,
    textWrap: "pretty",
  },
  link: {
    color: siteTokens.text,
    textDecorationColor: siteTokens.muted,
    textDecorationLine: "underline",
    textDecorationSkipInk: "auto",
    textDecorationThickness: "0.09em",
    textUnderlineOffset: "0.2em",
    transition: "text-decoration-color 150ms ease-out",
    ":hover": { textDecorationColor: "currentColor" },
  },
  figure: { margin: 0, minWidth: 0 },
  caption: {
    color: siteTokens.muted,
    fontSize: 13,
    lineHeight: 1.5,
    marginBottom: 14,
  },
  bar: { display: "flex", gap: 3 },
  // Each segment is as wide as its share of the skills.
  segment: { display: "flex", flexDirection: "column", gap: 10, minWidth: 0 },
  segmentSize: (count: number) => ({ flexBasis: 0, flexGrow: count }),
  count: {
    color: siteTokens.text,
    fontSize: 44,
    fontVariantNumeric: "tabular-nums",
    fontWeight: 640,
    letterSpacing: "-.03em",
    lineHeight: 1.05,
    [STACKED]: { fontSize: 32 },
  },
  fill: { borderRadius: 3, display: "block", height: 14 },
  fillColor: (color: string) => ({ backgroundColor: color }),
  // Hidden skills are not in the list at all, so their part of the bar is only an outline.
  fillHidden: {
    backgroundColor: "transparent",
    borderColor: siteTokens.muted,
    borderStyle: "dashed",
    borderWidth: 1,
  },
  tiers: { display: "flex", flexDirection: "column", margin: "22px 0 0" },
  tier: {
    alignItems: "baseline",
    borderTopColor: siteTokens.border,
    borderTopStyle: "solid",
    borderTopWidth: 1,
    columnGap: 24,
    display: "grid",
    gridTemplateColumns: "132px minmax(0, 1fr)",
    paddingBlock: 14,
    [STACKED]: { gridTemplateColumns: "minmax(0, 1fr)", rowGap: 4 },
  },
  term: {
    alignItems: "center",
    color: siteTokens.text,
    display: "flex",
    fontSize: 15,
    fontWeight: 640,
    gap: 8,
    whiteSpace: "nowrap",
  },
  dot: { borderRadius: 999, flexShrink: 0, height: 8, width: 8 },
  meaning: {
    color: siteTokens.muted,
    fontSize: 15,
    lineHeight: 1.55,
    margin: 0,
    maxWidth: "62ch",
    textWrap: "pretty",
  },
  visuallyHidden: {
    clip: "rect(0 0 0 0)",
    clipPath: "inset(50%)",
    height: 1,
    overflow: "hidden",
    position: "absolute",
    whiteSpace: "nowrap",
    width: 1,
  },
  proof: {
    borderTopColor: siteTokens.border,
    borderTopStyle: "solid",
    borderTopWidth: 1,
    color: siteTokens.muted,
    fontSize: 14,
    lineHeight: 1.55,
    margin: 0,
    paddingTop: 14,
    textWrap: "pretty",
  },
  // The rule above the text spans the figure. The text keeps a readable line length.
  proofText: { display: "block", maxWidth: "68ch" },
});
