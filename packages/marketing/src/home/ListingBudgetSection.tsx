import * as stylex from "@stylexjs/stylex";

import { siteTokens } from "../SiteTheme.stylex";
import { homeSectionStyles } from "./HomeSection.stylex";

const MEASURED_LISTINGS = [
  {
    agent: "Claude Code",
    needed: "70,845 of 75,000 characters",
    listing: "All 152 descriptions fit.",
    full: 152,
    shortened: 0,
  },
  {
    agent: "Codex",
    needed: "20,734 tokens needed. Budget of 10,000 tokens.",
    listing: "Codex shortened 124 of 165 descriptions.",
    full: 41,
    shortened: 124,
  },
];

/** Shows what fit in each agent’s skill list on one Mac. */
export function ListingBudgetSection() {
  return (
    <>
      <h2 {...stylex.props(homeSectionStyles.title, styles.title)}>
        What happens when skills don’t fit?
      </h2>
      <div {...stylex.props(homeSectionStyles.columns)}>
        <div {...stylex.props(styles.intro)}>
          <p {...stylex.props(styles.lead)}>
            Claude Code and Codex give the model a list of skills. When the list runs out of room,
            agents shorten or drop descriptions. That’s how they stay within budget, but with less
            detail, the model has a harder time picking the right skill.
          </p>
          <p {...stylex.props(styles.note)}>
            Park skills you rarely use to make room for the ones you need. Model listing shows what
            fits and why a skill stays hidden, for example because it’s manual-only or belongs to
            another project.
          </p>
        </div>
        <figure {...stylex.props(styles.figure)}>
          <figcaption {...stylex.props(styles.note)}>
            One Mac with 438 installed skills, measured in Skill Studio 0.4.1.
          </figcaption>
          <dl {...stylex.props(styles.agents)}>
            {MEASURED_LISTINGS.map(({ agent, needed, listing, full, shortened }) => (
              <div key={agent} {...stylex.props(styles.agent)}>
                <dt {...stylex.props(styles.agentName)}>{agent}</dt>
                <dd {...stylex.props(styles.detail)}>
                  <span>{needed}</span>
                  <div aria-hidden="true" {...stylex.props(styles.bar)}>
                    <span {...stylex.props(styles.segment, styles.share(full), styles.full)} />
                    {shortened > 0 && (
                      <span
                        {...stylex.props(styles.segment, styles.share(shortened), styles.shortened)}
                      />
                    )}
                  </div>
                  <span {...stylex.props(styles.outcome)}>{listing}</span>
                </dd>
              </div>
            ))}
          </dl>
        </figure>
      </div>
    </>
  );
}

const styles = stylex.create({
  title: { marginBottom: 36 },
  intro: {
    display: "flex",
    flexDirection: "column",
    gap: 16,
    "@media (max-width: 760px)": { marginBottom: 28 },
  },
  lead: { color: siteTokens.text, fontSize: 17, lineHeight: 1.55, margin: 0, textWrap: "pretty" },
  note: { color: siteTokens.muted, fontSize: 14, lineHeight: 1.55, margin: 0, textWrap: "pretty" },
  figure: { margin: 0, minWidth: 0 },
  agents: { display: "grid", gap: 18, margin: "16px 0 0" },
  agent: { borderTop: `1px solid ${siteTokens.border}`, paddingTop: 12 },
  agentName: { color: siteTokens.text, fontWeight: 640 },
  detail: { display: "grid", gap: 6, margin: "4px 0 0", color: siteTokens.muted, fontSize: 14 },
  bar: { display: "flex", gap: 3, marginBlock: 4 },
  segment: { borderRadius: 3, height: 12, flexBasis: 0 },
  share: (count: number) => ({ flexGrow: count }),
  full: { backgroundColor: "oklch(0.75 0.17 150)" },
  shortened: { backgroundColor: "oklch(0.8 0.15 80)" },
  outcome: { color: siteTokens.text },
});
