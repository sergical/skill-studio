import * as stylex from "@stylexjs/stylex";

import { DITHER_COLORS } from "../ClosingFinale";
import { WarpDither } from "../WarpDither";
import { ProductMock } from "../../ProductMock";
import { siteTokens } from "../../SiteTheme.stylex";
import { BaselineHero } from "./BaselineHero";
import { KineticCalculator } from "./KineticCalculator";
import { HomeShell, type HomePageProps } from "./HomeShell";

const MOBILE = "@media (max-width: 680px)";

function ImmersiveProduct({ theme, onToggleTheme }: HomePageProps) {
  return (
    <section id="product" {...stylex.props(styles.section)}>
      <div {...stylex.props(styles.frame, styles.frameBack(DITHER_COLORS[theme].back))}>
        <div aria-hidden="true" {...stylex.props(styles.dither)}>
          <WarpDither colorBack="#00000000" colorFront={DITHER_COLORS[theme].front} />
        </div>
        <ProductMock
          theme={theme}
          onToggleTheme={onToggleTheme}
          initialView="home"
          xstyle={styles.window}
        />
      </div>
    </section>
  );
}

export function HomePage(props: HomePageProps) {
  return (
    <HomeShell
      {...props}
      hero={<BaselineHero theme={props.theme} />}
      product={<ImmersiveProduct {...props} />}
      calculator={<KineticCalculator />}
    />
  );
}

const styles = stylex.create({
  section: {
    boxSizing: "border-box",
    marginInline: "auto",
    maxWidth: 1440,
    paddingBottom: 24,
    paddingInline: 28,
    position: "relative",
    scrollMarginTop: 72,
    width: "100%",
    zIndex: 1,
    "@media (max-width: 600px)": { paddingInline: 12 },
  },
  frame: {
    borderRadius: { default: 24, [MOBILE]: 16 },
    boxShadow: `0 0 0 1px ${siteTokens.border}`,
    isolation: "isolate",
    overflow: "clip",
    padding: "clamp(16px, 4.5vw, 64px)",
    position: "relative",
  },
  frameBack: (color: string) => ({ backgroundColor: color }),
  // Strongest along the top edge, so the window reads as lit from above.
  dither: {
    inset: 0,
    maskImage: "radial-gradient(ellipse 110% 85% at 50% 0%, black 30%, transparent 92%)",
    pointerEvents: "none",
    position: "absolute",
    zIndex: -1,
  },
  window: {
    boxShadow:
      "0 0 0 1px oklch(0 0 0 / .12), 0 2px 6px oklch(0 0 0 / .24), 0 32px 90px oklch(0 0 0 / .5)",
    height: { default: "clamp(600px, calc(100svh - 200px), 780px)", [MOBILE]: 560 },
    marginInline: "auto",
    maxWidth: 1240,
  },
});
