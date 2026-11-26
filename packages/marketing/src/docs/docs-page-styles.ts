// ============================================================================
// Skill Studio - Docs and 404 page styles
// Kept apart from the page components so docs-main.ts can pull these StyleX
// rules into the client CSS without bundling React
// ============================================================================

import * as stylex from "@stylexjs/stylex";

import { siteTokens } from "../SiteTheme.stylex";

export const docsPageStyles = stylex.create({
  layout: {
    display: "grid",
    gap: { default: 48, "@media (min-width: 1180px)": 40, "@media (max-width: 860px)": 28 },
    gridTemplateColumns: {
      default: "224px minmax(0, 1fr)",
      "@media (min-width: 1180px)": "224px minmax(0, 1fr) 200px",
      "@media (max-width: 860px)": "minmax(0, 1fr)",
    },
    paddingBlock: { default: "32px 96px", "@media (max-width: 860px)": "8px 72px" },
  },
  outline: {
    alignSelf: "start",
    display: { default: "none", "@media (min-width: 1180px)": "block" },
    maxHeight: "calc(100dvh - 48px)",
    overflowY: "auto",
    paddingTop: 8,
    position: "sticky",
    top: 24,
  },
  nav: {
    alignSelf: "start",
    display: "flex",
    flexDirection: "column",
    gap: 12,
    maxHeight: "calc(100dvh - 48px)",
    position: "sticky",
    top: 24,
    "@media (max-width: 860px)": { maxHeight: "none", position: "static" },
  },
  search: {
    appearance: "none",
    backgroundColor: siteTokens.surface,
    borderColor: siteTokens.border,
    borderStyle: "solid",
    borderWidth: 1,
    borderRadius: 8,
    color: siteTokens.text,
    // 16px on touch screens stops iOS from zooming into the field.
    fontSize: { default: 14, "@media (pointer: coarse)": 16 },
    minHeight: { default: 36, "@media (pointer: coarse)": 44 },
    paddingBlock: 6,
    paddingInline: "34px 30px",
    transition: "border-color 150ms ease-out",
    width: "100%",
    ":focus-visible": { outlineColor: siteTokens.text },
  },
  navHeading: {
    color: siteTokens.muted,
    fontSize: 12,
    fontWeight: 600,
    letterSpacing: "0.02em",
    lineHeight: "16px",
    marginBottom: 4,
    paddingInline: 10,
  },
  navLink: {
    borderRadius: 6,
    color: siteTokens.muted,
    display: "block",
    fontSize: 14,
    lineHeight: 1.25,
    minHeight: 32,
    paddingBlock: 7,
    paddingInline: 10,
    textDecoration: "none",
    transition: "background-color 150ms ease-out, color 150ms ease-out",
    "@media (pointer: coarse)": { minHeight: 44, paddingBlock: 13 },
  },
  navLinkCurrent: {
    backgroundColor: siteTokens.accentSoft,
    color: siteTokens.text,
  },
  // Use the same site theme for the prerendered Markdown and docs controls.
  colors: {
    "--docs-accent": siteTokens.accent,
    "--docs-border": siteTokens.border,
    "--docs-accent-soft": siteTokens.accentSoft,
    "--docs-background": siteTokens.background,
    "--docs-muted": siteTokens.muted,
    "--docs-surface": siteTokens.surface,
    "--docs-text": siteTokens.text,
  },
  article: {
    maxWidth: 720,
    minWidth: 0,
  },
});

export const notFoundStyles = stylex.create({
  main: {
    display: "flex",
    flexDirection: "column",
    gap: 16,
    minHeight: "60vh",
    justifyContent: "center",
    paddingBlock: 96,
  },
  code: { color: siteTokens.muted, fontSize: 14, margin: 0 },
  title: {
    fontSize: "clamp(36px, 6vw, 56px)",
    fontWeight: 600,
    letterSpacing: "-0.03em",
    lineHeight: 1.05,
    margin: 0,
  },
  text: { color: siteTokens.muted, fontSize: 17, lineHeight: 1.6, margin: 0, maxWidth: 520 },
  links: { display: "flex", flexWrap: "wrap", gap: 12, marginTop: 12 },
  link: {
    alignItems: "center",
    borderColor: siteTokens.border,
    borderRadius: 10,
    borderStyle: "solid",
    borderWidth: 1,
    color: siteTokens.text,
    display: "inline-flex",
    fontSize: 14,
    minHeight: 44,
    paddingInline: 16,
    textDecoration: "none",
    transition: "border-color 150ms ease-out",
    ":hover": { borderColor: siteTokens.muted },
  },
});
