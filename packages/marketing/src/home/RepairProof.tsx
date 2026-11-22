import { useSyncExternalStore } from "react";
import * as stylex from "@stylexjs/stylex";
import { Check, Copy, Link2 } from "lucide-react";

import { siteTokens, type SiteTheme } from "../SiteTheme.stylex";
import { homeSectionStyles } from "./HomeSection.stylex";
import { useCopyFlash } from "./useCopyFlash";

const DIAGNOSE_COMMAND = "npx -y skill-studio@latest diagnose";
const COMPACT = "(max-width: 760px)";

function subscribeToCompactLayout(onChange: () => void) {
  const query = matchMedia(COMPACT);
  query.addEventListener("change", onChange);
  return () => query.removeEventListener("change", onChange);
}

function RepairWalkthrough({ theme }: { theme: SiteTheme }) {
  const compact = useSyncExternalStore(
    subscribeToCompactLayout,
    () => matchMedia(COMPACT).matches,
    () => false,
  );
  const asset = `/walkthrough/current/repair-${theme}-${compact ? "mobile" : "desktop"}`;
  return (
    <figure {...stylex.props(styles.walkthrough)}>
      <figcaption {...stylex.props(styles.scan)}>
        <strong>Home finds a broken link.</strong> The Claude Code copy of{" "}
        <code {...stylex.props(homeSectionStyles.code)}>release-notes</code> points to a folder
        that’s gone. Review its new location, then choose <strong>Fix it</strong>.
      </figcaption>
      <video
        aria-label="Preview and fix a broken skill link"
        controls
        muted
        playsInline
        preload="none"
        poster={`${asset}.jpg`}
        src={`${asset}.mp4`}
        {...stylex.props(styles.video, compact && styles.compactVideo)}
      />
      <div {...stylex.props(styles.history)}>
        <span {...stylex.props(styles.historyPath)}>Activity → History</span>
        <div {...stylex.props(styles.historyRow)}>
          <Link2 aria-hidden="true" size={16} />
          <strong>Link repaired</strong>
          <code {...stylex.props(homeSectionStyles.code)}>release-notes</code>
        </div>
      </div>
    </figure>
  );
}

/** Shows a broken link repair and the command to check skills from a terminal. */
export function RepairProof({ theme }: { theme: SiteTheme }) {
  const { copied, copy } = useCopyFlash();
  return (
    <>
      <h2 {...stylex.props(homeSectionStyles.title)}>See a broken link fixed</h2>
      <div {...stylex.props(styles.columns)}>
        <RepairWalkthrough theme={theme} />
        <div>
          <h3 {...stylex.props(styles.terminalTitle)}>Check from your terminal</h3>
          <div {...stylex.props(styles.terminal)}>
            <code {...stylex.props(homeSectionStyles.code)}>{DIAGNOSE_COMMAND}</code>
            <button
              type="button"
              aria-label={copied ? "Command copied" : "Copy diagnose command"}
              onClick={() => copy(DIAGNOSE_COMMAND)}
              {...stylex.props(styles.copy)}
            >
              {copied ? (
                <Check size={16} aria-hidden="true" />
              ) : (
                <Copy size={16} aria-hidden="true" />
              )}
            </button>
          </div>
          <p {...stylex.props(styles.explanation)}>
            It lists the skills it finds and reports problems. With no skills installed, it prints
            “No skills found.” and “No issues.”
          </p>
        </div>
      </div>
    </>
  );
}

const styles = stylex.create({
  columns: {
    display: "grid",
    gap: 40,
    gridTemplateColumns: "minmax(0, 3fr) minmax(0, 2fr)",
    "@media (max-width: 760px)": { gap: 24, gridTemplateColumns: "minmax(0, 1fr)" },
  },
  walkthrough: { margin: 0 },
  scan: { color: siteTokens.muted, lineHeight: 1.55, marginBottom: 14 },
  video: {
    aspectRatio: "16 / 9",
    backgroundColor: siteTokens.surface,
    border: `1px solid ${siteTokens.border}`,
    borderRadius: 12,
    display: "block",
    width: "100%",
  },
  compactVideo: { aspectRatio: "1 / 1" },
  history: { borderBottom: `1px solid ${siteTokens.border}`, paddingBlock: 13 },
  historyPath: { color: siteTokens.muted, fontSize: 12 },
  historyRow: {
    alignItems: "center",
    display: "flex",
    flexWrap: "wrap",
    fontSize: 14,
    gap: 10,
    marginTop: 6,
  },
  terminalTitle: { fontSize: 17, fontWeight: 640, margin: "0 0 12px" },
  terminal: {
    alignItems: "center",
    backgroundColor: siteTokens.surface,
    border: `1px solid ${siteTokens.border}`,
    borderRadius: 10,
    display: "flex",
    gap: 8,
    justifyContent: "space-between",
    paddingInlineStart: 12,
  },
  copy: {
    backgroundColor: "transparent",
    borderWidth: 0,
    color: { default: siteTokens.muted, ":hover": siteTokens.text },
    cursor: "pointer",
    flexShrink: 0,
    height: 44,
    width: 44,
  },
  explanation: { color: siteTokens.muted, fontSize: 14, lineHeight: 1.55, margin: "10px 0 0" },
});
