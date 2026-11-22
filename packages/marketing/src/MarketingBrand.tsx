import * as stylex from "@stylexjs/stylex";

import { paletteVars } from "./PaletteThemes.stylex";
import { siteTokens } from "./SiteTheme.stylex";

interface LogoLockupProps {
  href: string;
  compact?: boolean;
}

export function LogoLockup({ href, compact = false }: LogoLockupProps) {
  return (
    <a href={href} {...stylex.props(stylex.defaultMarker(), brandStyles.lockup)}>
      <img
        src="/skill-studio-logo-128.webp"
        alt=""
        width={compact ? 32 : 38}
        height={compact ? 32 : 38}
        {...stylex.props(brandStyles.logo)}
      />
      <span {...stylex.props(brandStyles.wordmark, compact && brandStyles.compactWordmark)}>
        Skill Studio
      </span>
    </a>
  );
}

const brandStyles = stylex.create({
  lockup: {
    alignItems: "center",
    color: siteTokens.text,
    display: "inline-flex",
    gap: 9,
    textDecoration: "none",
  },
  logo: {
    display: "block",
    filter: {
      default: paletteVars.logoFilter,
      [stylex.when.ancestor(":focus-visible")]: "none",
      "@media (hover: hover) and (pointer: fine)": {
        [stylex.when.ancestor(":hover")]: "none",
      },
    },
    height: "auto",
    objectFit: "contain",
    transition: "filter 180ms ease, transform 300ms cubic-bezier(0.19, 1, 0.22, 1)",
    "@media (hover: hover) and (pointer: fine)": {
      ":hover": { transform: "rotate(-6deg) scale(1.06)" },
    },
    "@media (prefers-reduced-motion: reduce)": {
      transition: "none",
      ":hover": { transform: "none" },
    },
  },
  wordmark: {
    fontSize: 15,
    fontWeight: 680,
    letterSpacing: "-0.025em",
  },
  compactWordmark: {
    fontSize: 14,
  },
});
