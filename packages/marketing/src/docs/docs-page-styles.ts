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
    gap: 56,
    gridTemplateColumns: "200px minmax(0, 1fr)",
    paddingBlock: "32px 96px",
    "@media (max-width: 860px)": {
      gap: 28,
      gridTemplateColumns: "minmax(0, 1fr)",
      paddingBlock: "8px 72px",
    },
  },
  nav: {
    alignSelf: "start",
    display: "flex",
    flexDirection: "column",
    gap: 2,
    maxHeight: "calc(100dvh - 48px)",
    overflowY: "auto",
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
    fontSize: 16,
    marginBottom: 8,
    minHeight: 44,
    padding: "8px 12px",
    width: "100%",
  },
  navHeading: {
    color: siteTokens.muted,
    fontSize: 12,
    fontWeight: 600,
    letterSpacing: "0.04em",
    marginBlock: "12px 4px",
    paddingInline: 12,
    textTransform: "uppercase",
  },
  navLink: {
    borderRadius: 8,
    color: siteTokens.muted,
    display: "block",
    fontSize: 14,
    minHeight: 44,
    paddingBlock: 11,
    paddingInline: 12,
    textDecoration: "none",
    transition: "color 150ms ease-out, background-color 150ms ease-out",
    ":hover": { color: siteTokens.text },
  },
  navLinkCurrent: {
    backgroundColor: siteTokens.surface,
    color: siteTokens.text,
  },
  // Use the same site theme for the prerendered Markdown and docs controls.
  colors: {
    "--docs-accent": siteTokens.accent,
    "--docs-border": siteTokens.border,
    "--docs-muted": siteTokens.muted,
    "--docs-surface": siteTokens.surface,
    "--docs-text": siteTokens.text,
  },
  article: {
    maxWidth: 720,
    minWidth: 0,
  },
  markdownLink: {
    color: siteTokens.muted,
    display: "inline-block",
    fontSize: 13,
    textDecoration: "underline",
    textUnderlineOffset: 3,
    ":hover": { color: siteTokens.text },
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
