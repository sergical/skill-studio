// PROTO(home-calc) variant "Baseline Plus+": the Baseline Plus hero, then the product preview
// right under it in a large dither frame, opening on Home. The calculator is Kinetic's dot
// field with Claude Code's skillListingBudgetFraction and Codex's skills.max_context_tokens.
import * as stylex from "@stylexjs/stylex";

import { DITHER_COLORS } from "../../home/ClosingFinale";
import { WarpDither } from "../../home/WarpDither";
import { ProductMock } from "../../ProductMock";
import { siteTokens } from "../../SiteTheme.stylex";
import { BaselineHero } from "./BaselinePlus";
import { KineticCalculator } from "./Kinetic";
import { ProtoPage, type VariantProps } from "./ProtoPage";

const MOBILE = "@media (max-width: 680px)";

function ImmersiveProduct({ theme, onToggleTheme }: VariantProps) {
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

export function BaselinePlusPlus(props: VariantProps) {
  return (
    <ProtoPage
      {...props}
      hero={<BaselineHero theme={props.theme} />}
      product={<ImmersiveProduct {...props} />}
      calculator={<KineticCalculator budgets />}
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
