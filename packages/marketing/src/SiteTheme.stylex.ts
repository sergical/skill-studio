import * as stylex from "@stylexjs/stylex";

import { paletteVars } from "./PaletteThemes.stylex";

export type SiteTheme = "dark" | "light";

export const siteTokens = stylex.defineVars({
  accent: paletteVars.darkAccent,
  accentHover: paletteVars.darkAccentHover,
  accentSoft: paletteVars.darkAccentSoft,
  accentText: paletteVars.darkAccentText,
  background: paletteVars.darkBackground,
  border: paletteVars.darkBorder,
  muted: paletteVars.darkMuted,
  success: "oklch(0.75 0.17 150)",
  surface: paletteVars.darkSurface,
  text: paletteVars.darkText,
  warning: "oklch(0.8 0.15 80)",
});

export const lightSiteTheme = stylex.createTheme(siteTokens, {
  accent: paletteVars.lightAccent,
  accentHover: paletteVars.lightAccentHover,
  accentSoft: paletteVars.lightAccentSoft,
  accentText: paletteVars.lightAccentText,
  background: paletteVars.lightBackground,
  border: paletteVars.lightBorder,
  muted: paletteVars.lightMuted,
  success: "oklch(0.46 0.16 150)",
  surface: paletteVars.lightSurface,
  text: paletteVars.lightText,
  warning: "oklch(0.64 0.16 75)",
});
