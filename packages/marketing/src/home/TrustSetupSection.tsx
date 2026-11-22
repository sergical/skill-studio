import * as stylex from "@stylexjs/stylex";

import { siteTokens } from "../SiteTheme.stylex";
import { homeSectionStyles } from "./HomeSection.stylex";

/** Explains disk changes, recovery, connections, and the three ways to get started. */
export function TrustSetupSection() {
  return (
    <div {...stylex.props(homeSectionStyles.columns)}>
      <h2 {...stylex.props(homeSectionStyles.title)}>Trust and setup</h2>
      <div>
        <dl {...stylex.props(styles.facts)}>
          <div>
            <dt {...stylex.props(styles.label)}>Files on your Mac</dt>
            <dd {...stylex.props(styles.text)}>
              Scanning reads skill files. Fixing, installing, updating, parking, and removing write
              to skill folders. Changes in a project can show up in Git.
            </dd>
          </div>
          <div>
            <dt {...stylex.props(styles.label)}>Undo</dt>
            <dd {...stylex.props(styles.text)}>
              Unpark a skill to bring it back. Activity history supports undo for installs, updates,
              removes, and repairs. Restore can refuse after later edits or when a backup has
              expired.
            </dd>
          </div>
          <div>
            <dt {...stylex.props(styles.label)}>Network</dt>
            <dd {...stylex.props(styles.text)}>
              The Skill Store, public install counts, installs, updates, and update checks use the
              network. Skill update checks, copying from GitHub, and pulling upstream changes need a
              signed-in <code {...stylex.props(homeSectionStyles.code)}>gh</code>.
            </dd>
          </div>
          <div>
            <dt {...stylex.props(styles.label)}>Telemetry</dt>
            <dd {...stylex.props(styles.text)}>
              Crash and timing reports are on by default in desktop setup. Turn off Send crash and
              performance data in Settings under Telemetry.
            </dd>
          </div>
        </dl>
        <ul aria-label="Setup links" {...stylex.props(styles.links)}>
          <li>
            <a href="/docs/#install" {...stylex.props(homeSectionStyles.textLink)}>
              Desktop app <span aria-hidden="true">→</span>
            </a>
          </li>
          <li>
            <a href="/docs/cli/#run-it" {...stylex.props(homeSectionStyles.textLink)}>
              CLI <span aria-hidden="true">→</span>
            </a>
          </li>
          <li>
            <a
              href="/docs/mcp/#let-your-agent-set-it-up"
              {...stylex.props(homeSectionStyles.textLink)}
            >
              MCP server <span aria-hidden="true">→</span>
            </a>
          </li>
        </ul>
      </div>
    </div>
  );
}

const styles = stylex.create({
  facts: { display: "grid", gap: 14, margin: 0 },
  label: { fontSize: 16, fontWeight: 640 },
  text: {
    color: siteTokens.muted,
    fontSize: 15,
    lineHeight: 1.55,
    margin: "4px 0 0",
    maxWidth: "68ch",
  },
  links: {
    display: "flex",
    flexWrap: "wrap",
    gap: "0 24px",
    listStyle: "none",
    margin: "14px 0 0",
    padding: 0,
  },
});
