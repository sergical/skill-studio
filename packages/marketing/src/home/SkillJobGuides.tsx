import * as stylex from "@stylexjs/stylex";

import { siteTokens } from "../SiteTheme.stylex";
import { homeSectionStyles } from "./HomeSection.stylex";

const INSTALL_GUIDE = {
  title: "Install and update",
  jobs: [
    {
      id: "install-skills",
      title: "Choose what to install",
      steps:
        "Open Add skill and browse skills.sh or paste a repository link. Pick the skills and agents you want, then install them together.",
    },
    {
      id: "update-skills",
      title: "Check for updates",
      steps:
        "Review updates on Home and apply them one at a time or together. For a pack, open Packs, click Check upstream, and review what will change before you update.",
    },
  ],
};

const CUSTOMIZE_GUIDE = {
  title: "Customize and share",
  jobs: [
    {
      id: "choose-skills-per-agent",
      title: "Turn a skill off for one agent",
      steps:
        "Open the skill and turn off the agent you want to exclude. If it shares a copy with other agents, Skill Studio previews the split before it parks that agent’s copy.",
    },
    {
      id: "fork-skills",
      title: "Edit a skill and keep its source",
      steps:
        "Fork a shared global skills.sh skill and edit your copy. Use Pull upstream to review and bring in changes from the original.",
    },
    {
      id: "share-skills",
      title: "Export a pack you can share",
      steps:
        "Select skills on Home or Skills and choose Pack. Export creates a folder with the skills and install instructions. Put the folder in a Git repository to share it.",
    },
  ],
};

/** Shows where to do each install, update, and customization job in the app. */
export function SkillJobGuide({ kind }: { kind: "install" | "customize" }) {
  const { title, jobs } = kind === "install" ? INSTALL_GUIDE : CUSTOMIZE_GUIDE;
  return (
    <div {...stylex.props(homeSectionStyles.columns)}>
      <h2 {...stylex.props(homeSectionStyles.title)}>{title}</h2>
      <dl {...stylex.props(styles.jobs)}>
        {jobs.map((job) => (
          <div key={job.id} id={job.id} {...stylex.props(styles.job)}>
            <dt {...stylex.props(styles.title)}>{job.title}</dt>
            <dd {...stylex.props(styles.steps)}>{job.steps}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

const styles = stylex.create({
  jobs: { margin: 0 },
  job: {
    borderTop: `1px solid ${siteTokens.border}`,
    display: "grid",
    gap: 6,
    paddingBlock: 14,
    scrollMarginTop: 80,
  },
  title: { color: siteTokens.text, fontSize: 16, fontWeight: 640 },
  steps: { color: siteTokens.muted, lineHeight: 1.55, margin: 0, maxWidth: "66ch" },
});
