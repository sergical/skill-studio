import { useRef } from "react";
import * as stylex from "@stylexjs/stylex";

import { siteLayout } from "../../SiteChrome";
import { siteTokens, type SiteTheme } from "../../SiteTheme.stylex";
import { HERO_SUBLINE, HERO_TITLE } from "./hero-copy";
import {
  CopyAgentPromptButton,
  DownloadButton,
  HeroDither,
  TrustLine,
  heroGlow,
  heroPartStyles,
} from "./HeroParts";
import { SkillThrow } from "./pile/SkillThrow";
import { useHeroPointer } from "./useHeroPointer";

export function BaselineHero({ theme }: { theme: SiteTheme }) {
  const rootRef = useRef<HTMLDivElement>(null);
  const layerRef = useRef<HTMLDivElement>(null);
  useHeroPointer({ root: rootRef, vars: layerRef });
  return (
    <div ref={rootRef} {...stylex.props(styles.heroWrap, heroGlow.baseline)}>
      <HeroDither theme={theme} layerRef={layerRef} />
      <section {...stylex.props(siteLayout.container, styles.hero)}>
        <div {...stylex.props(styles.copy)}>
          <h1 {...stylex.props(styles.title)}>{HERO_TITLE}</h1>
          <p {...stylex.props(styles.lede)}>{HERO_SUBLINE}</p>
          <div {...stylex.props(heroPartStyles.actions)}>
            <DownloadButton place="hero" />
            <CopyAgentPromptButton />
          </div>
          <TrustLine />
        </div>
        <div {...stylex.props(styles.visual)}>
          <SkillThrow />
        </div>
      </section>
    </div>
  );
}

const styles = stylex.create({
  heroWrap: { isolation: "isolate", position: "relative" },
  hero: {
    alignItems: "center",
    display: "grid",
    gap: "clamp(32px,4vw,64px)",
    gridTemplateColumns: "minmax(0,.9fr) minmax(0,1.1fr)",
    minHeight: 460,
    paddingBlock: "36px 48px",
    "@media (max-width: 1080px)": { gridTemplateColumns: "1fr", justifyItems: "center" },
    "@media (max-width: 600px)": { gap: 28, paddingBlock: "28px 40px" },
  },
  copy: {
    maxWidth: 560,
    "@media (max-width: 1080px)": {
      alignItems: "center",
      display: "flex",
      flexDirection: "column",
      textAlign: "center",
      width: "100%",
    },
  },
  title: {
    fontSize: "clamp(44px,5.4vw,80px)",
    fontWeight: 700,
    letterSpacing: "-.025em",
    lineHeight: 1,
    margin: 0,
    textWrap: "balance",
  },
  lede: {
    color: siteTokens.muted,
    fontSize: 18,
    lineHeight: 1.6,
    margin: "24px 0 30px",
    maxWidth: "40ch",
    textWrap: "pretty",
    "@media (max-width: 600px)": { fontSize: 16 },
  },
  visual: {
    minWidth: 0,
    position: "relative",
    width: "100%",
    "@media (max-width: 1080px)": { maxWidth: 680 },
  },
});
