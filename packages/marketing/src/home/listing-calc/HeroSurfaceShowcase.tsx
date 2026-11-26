// ============================================================================
// Skill Studio - Hero Surface Showcase (Desktop App · CLI · MCP Server)
// Prominent floating glass tabs + live-synced top command bar, smooth jump-free
// height interpolation via ResizeObserver, and surface-specific docs link below.
// ============================================================================
import { useLayoutEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import * as stylex from "@stylexjs/stylex";
import { AGENT_SETUP_PROMPT } from "@skill-studio/lib/agent-setup";
import {
  AppWindow,
  Check,
  Copy,
  Download,
  Plug,
  Share,
  Terminal,
  type LucideIcon,
} from "lucide-react";

import { ProductMock, type ThemeToggleOrigin } from "../../ProductMock";
import { siteTokens, type SiteTheme } from "../../SiteTheme.stylex";
import { CLI_DOCS_URL, DOCS_URL, DOWNLOAD_URL, MCP_DOCS_URL, SITE_URL } from "../../site-links";
import { DITHER_COLORS } from "../ClosingFinale";
import { WarpDither } from "../WarpDither";
import { useCopyFlash } from "../useCopyFlash";
import { CLI_COMMANDS, HeroCliWindow, HeroMcpWindow, type CliCommandId } from "./HeroSurfacePanels";

export type HeroSurfaceId = "app" | "cli" | "mcp";

const SURFACE_ORDER = {
  app: 0,
  cli: 1,
  mcp: 2,
} satisfies Record<HeroSurfaceId, number>;

const SURFACES: ReadonlyArray<{
  id: HeroSurfaceId;
  label: string;
  mobileLabel: string;
  icon: LucideIcon;
  docsHref: string;
  docsLabel: string;
}> = [
  {
    id: "app",
    label: "Desktop App",
    mobileLabel: "App",
    icon: AppWindow,
    docsHref: DOCS_URL,
    docsLabel: "Read getting started docs",
  },
  {
    id: "cli",
    label: "CLI",
    mobileLabel: "CLI",
    icon: Terminal,
    docsHref: CLI_DOCS_URL,
    docsLabel: "Read CLI docs",
  },
  {
    id: "mcp",
    label: "MCP Server",
    mobileLabel: "MCP",
    icon: Plug,
    docsHref: MCP_DOCS_URL,
    docsLabel: "Read MCP docs",
  },
];

const ARROW_STEPS = new Map([
  ["ArrowRight", 1],
  ["ArrowDown", 1],
  ["ArrowLeft", -1],
  ["ArrowUp", -1],
]);

function SmoothHeightStage({
  activeKey,
  direction,
  instant,
  theme,
  children,
}: {
  activeKey: HeroSurfaceId;
  direction: "forward" | "backward" | "none";
  instant: boolean;
  theme: SiteTheme;
  children: ReactNode;
}) {
  const outerRef = useRef<HTMLDivElement>(null);
  const innerRef = useRef<HTMLDivElement>(null);
  const hasMeasured = useRef(false);
  const instantRef = useRef(instant);

  useLayoutEffect(() => {
    instantRef.current = instant;
  }, [instant]);

  useLayoutEffect(() => {
    const outer = outerRef.current;
    const inner = innerRef.current;
    if (!outer || !inner) return;

    const sync = (snap: boolean) => {
      const next = Math.ceil(inner.getBoundingClientRect().height);
      if (next <= 0) return;
      if (snap || !hasMeasured.current || instantRef.current) {
        outer.style.transition = "none";
        outer.style.height = `${next}px`;
        void outer.offsetHeight;
        outer.style.transition = "";
        hasMeasured.current = true;
        return;
      }
      outer.style.height = `${next}px`;
    };

    sync(!hasMeasured.current);

    if (!("ResizeObserver" in window)) return;
    const observer = new ResizeObserver(() => sync(false));
    observer.observe(inner);

    const onResize = () => sync(true);
    window.addEventListener("resize", onResize);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", onResize);
    };
  }, []);

  return (
    <div
      ref={outerRef}
      {...stylex.props(
        styles.stageOuter,
        activeKey === "app" ? styles.stageAppRadius : styles.stagePanelRadius,
        theme === "light" ? styles.stageShadowLight : styles.stageShadowDark,
      )}
    >
      <div ref={innerRef}>
        <div
          key={activeKey}
          {...stylex.props(
            !instant && direction === "forward" && styles.paneForward,
            !instant && direction === "backward" && styles.paneBackward,
          )}
        >
          {children}
        </div>
      </div>
    </div>
  );
}

export function HeroSurfaceShowcase({
  theme,
  onToggleTheme,
}: {
  theme: SiteTheme;
  onToggleTheme: (origin: ThemeToggleOrigin) => void;
}) {
  const [surface, setSurface] = useState<HeroSurfaceId>("app");
  const [direction, setDirection] = useState<"forward" | "backward" | "none">("none");
  const [instant, setInstant] = useState(false);
  const [cliCmdId, setCliCmdId] = useState<CliCommandId>("listing");
  const { copiedText, copy } = useCopyFlash();

  // Sliding pill measurement
  const tabRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const pillRef = useRef<HTMLSpanElement>(null);
  const hasPositioned = useRef(false);
  const viaKeyboardRef = useRef(false);
  const activeIndex = SURFACES.findIndex((s) => s.id === surface);
  const activeSurface = SURFACES[activeIndex] ?? SURFACES[0];
  const activeCli = CLI_COMMANDS.find((c) => c.id === cliCmdId) ?? CLI_COMMANDS[0];

  useLayoutEffect(() => {
    const syncPill = (animate: boolean) => {
      const btn = tabRefs.current[activeIndex];
      const pill = pillRef.current;
      if (!btn || !pill) return;
      pill.style.transition = animate ? "" : "none";
      pill.style.width = `${btn.offsetWidth}px`;
      pill.style.height = `${btn.offsetHeight}px`;
      pill.style.transform = `translate(${btn.offsetLeft}px, ${btn.offsetTop}px)`;
      pill.style.opacity = "1";
    };
    const animate = hasPositioned.current && !viaKeyboardRef.current;
    viaKeyboardRef.current = false;
    syncPill(animate);
    hasPositioned.current = true;
    const onResize = () => syncPill(false);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, [activeIndex]);

  const selectSurface = (next: HeroSurfaceId, viaKeyboard: boolean) => {
    if (next === surface) return;
    setInstant(viaKeyboard);
    setDirection(SURFACE_ORDER[next] > SURFACE_ORDER[surface] ? "forward" : "backward");
    setSurface(next);
  };

  const onTabKeyDown = (event: KeyboardEvent) => {
    const step = ARROW_STEPS.get(event.key);
    if (!step) return;
    event.preventDefault();
    viaKeyboardRef.current = true;
    const nextIdx = (activeIndex + step + SURFACES.length) % SURFACES.length;
    selectSurface(SURFACES[nextIdx].id, true);
    tabRefs.current[nextIdx]?.focus();
  };

  const sendToMac = () => {
    if (!("share" in navigator)) {
      copy(SITE_URL);
      return;
    }
    navigator
      .share({ title: "Skill Studio", text: "Skill Studio for Mac", url: SITE_URL })
      .catch(() => {});
  };
  const light = theme === "light";
  const copied =
    copiedText ===
    (surface === "cli" ? activeCli.cmd : surface === "mcp" ? AGENT_SETUP_PROMPT : SITE_URL);

  const bottomSummary =
    surface === "app"
      ? "Interactive macOS app preview. Inspect skills, check listing budgets, or switch views."
      : surface === "cli"
        ? `${activeCli.summary}. Runs in any shell with zero install.`
        : "Local stdio MCP server. Works with Claude Code, Codex, Cursor, OpenCode, and pi.";

  return (
    <section id="product" {...stylex.props(styles.section)}>
      <div {...stylex.props(styles.frame, styles.frameBack(DITHER_COLORS[theme].back))}>
        <div aria-hidden="true" {...stylex.props(styles.dither)}>
          <WarpDither colorBack="#00000000" colorFront={DITHER_COLORS[theme].front} />
        </div>

        {/* Prominent top bar: high-contrast surface switcher + primary command/action */}
        <div {...stylex.props(styles.topBar)}>
          <div
            role="tablist"
            aria-label="Skill Studio surfaces"
            onKeyDown={onTabKeyDown}
            {...stylex.props(styles.tabTrack, light && styles.tabTrackLight)}
          >
            <span
              ref={pillRef}
              aria-hidden="true"
              {...stylex.props(styles.tabPill, light && styles.tabPillLight)}
            />
            {SURFACES.map((item, idx) => {
              const Icon = item.icon;
              const selected = item.id === surface;
              return (
                <button
                  key={item.id}
                  ref={(el) => {
                    tabRefs.current[idx] = el;
                  }}
                  type="button"
                  role="tab"
                  id={`surface-tab-${item.id}`}
                  aria-selected={selected}
                  aria-controls="surface-panel"
                  tabIndex={selected ? 0 : -1}
                  onClick={() => selectSurface(item.id, false)}
                  {...stylex.props(
                    styles.tabBtn,
                    light && styles.tabBtnLight,
                    selected && (light ? styles.tabBtnOnLight : styles.tabBtnOn),
                  )}
                >
                  <Icon size={16} aria-hidden="true" />
                  <span {...stylex.props(styles.desktopLabel)}>{item.label}</span>
                  <span {...stylex.props(styles.mobileLabel)}>{item.mobileLabel}</span>
                </button>
              );
            })}
          </div>

          <div {...stylex.props(styles.topActionSlot)}>
            {surface === "app" ? (
              <>
                <a
                  href={DOWNLOAD_URL}
                  {...stylex.props(
                    styles.topActionLink,
                    light && styles.barLight,
                    styles.pointerOnly,
                  )}
                >
                  <Download size={15} aria-hidden="true" />
                  <span>Download for macOS</span>
                  <span {...stylex.props(styles.topActionMeta, light && styles.topActionMetaLight)}>
                    macOS 13+
                  </span>
                </a>
                <button
                  type="button"
                  onClick={sendToMac}
                  {...stylex.props(
                    styles.topActionLink,
                    light && styles.barLight,
                    copied && styles.commandBarCopied,
                    styles.touchOnly,
                  )}
                >
                  {copied ? (
                    <Check size={15} aria-hidden="true" />
                  ) : (
                    <Share size={15} aria-hidden="true" />
                  )}
                  <span>{copied ? "Link copied" : "Send to my Mac"}</span>
                </button>
              </>
            ) : surface === "cli" ? (
              <button
                type="button"
                aria-label={copied ? "Copied command" : `Copy command: ${activeCli.cmd}`}
                onClick={() => copy(activeCli.cmd)}
                {...stylex.props(
                  styles.commandBar,
                  light && styles.barLight,
                  copied && styles.commandBarCopied,
                )}
              >
                <span
                  {...stylex.props(styles.commandPrompt, light && styles.commandPromptLight)}
                  aria-hidden="true"
                >
                  $
                </span>
                <code {...stylex.props(styles.commandCode, light && styles.barTextLight)}>
                  {activeCli.cmd}
                </code>
                <span
                  {...stylex.props(
                    styles.copyBadge,
                    light && styles.copyBadgeLight,
                    copied && styles.copyBadgeCopied,
                    copied && light && styles.copyBadgeCopiedLight,
                  )}
                >
                  {copied ? (
                    <Check size={14} aria-hidden="true" />
                  ) : (
                    <Copy size={14} aria-hidden="true" />
                  )}
                  <span>{copied ? "Copied" : "Copy"}</span>
                </span>
              </button>
            ) : (
              <button
                type="button"
                aria-label={copied ? "Copied MCP setup prompt" : "Copy MCP setup prompt"}
                onClick={() => copy(AGENT_SETUP_PROMPT)}
                {...stylex.props(
                  styles.commandBar,
                  light && styles.barLight,
                  copied && styles.commandBarCopied,
                )}
              >
                <Plug
                  size={15}
                  aria-hidden="true"
                  {...stylex.props(styles.commandPrompt, light && styles.commandPromptLight)}
                />
                <span {...stylex.props(styles.promptPreview, light && styles.barTextLight)}>
                  “Set up Skill Studio MCP and check my skills…”
                </span>
                <span
                  {...stylex.props(
                    styles.copyBadge,
                    light && styles.copyBadgeLight,
                    copied && styles.copyBadgeCopied,
                    copied && light && styles.copyBadgeCopiedLight,
                  )}
                >
                  {copied ? (
                    <Check size={14} aria-hidden="true" />
                  ) : (
                    <Copy size={14} aria-hidden="true" />
                  )}
                  <span>{copied ? "Copied" : "Copy prompt"}</span>
                </span>
              </button>
            )}
            <span aria-live="polite" {...stylex.props(styles.visuallyHidden)}>
              {copied
                ? surface === "cli"
                  ? "Copied command"
                  : surface === "mcp"
                    ? "Copied MCP setup prompt"
                    : "Copied the link to Skill Studio"
                : ""}
            </span>
          </div>
        </div>

        {/* Main interactive window with jump-free height interpolation */}
        <div id="surface-panel" role="tabpanel" aria-labelledby={`surface-tab-${surface}`}>
          <SmoothHeightStage
            activeKey={surface}
            direction={direction}
            instant={instant}
            theme={theme}
          >
            {surface === "app" ? (
              <ProductMock
                theme={theme}
                onToggleTheme={onToggleTheme}
                initialView="home"
                xstyle={styles.appWindow}
              />
            ) : surface === "cli" ? (
              <HeroCliWindow activeId={cliCmdId} onSelectCommand={setCliCmdId} />
            ) : (
              <HeroMcpWindow />
            )}
          </SmoothHeightStage>
        </div>

        {/* Clean bottom bar: surface context on left, surface docs link on right */}
        <div {...stylex.props(styles.bottomBar)}>
          <span {...stylex.props(styles.bottomSummary)}>{bottomSummary}</span>
          <a href={activeSurface.docsHref} {...stylex.props(styles.bottomDocsLink)}>
            <span>{activeSurface.docsLabel}</span>
            <span aria-hidden="true">→</span>
          </a>
        </div>
      </div>
    </section>
  );
}

const MOBILE = "@media (max-width: 680px)";
const HOVER = "@media (hover: hover) and (pointer: fine)";
const TOUCH_DEVICE = "@media (hover: none) and (pointer: coarse)";
const REDUCED = "@media (prefers-reduced-motion: reduce)";
const PRESS_EASE = "cubic-bezier(0.25, 0.46, 0.45, 0.94)";

const enterFromRight = stylex.keyframes({
  from: { opacity: 0, transform: "translateX(10px)" },
  to: { opacity: 1, transform: "translateX(0)" },
});

const enterFromLeft = stylex.keyframes({
  from: { opacity: 0, transform: "translateX(-10px)" },
  to: { opacity: 1, transform: "translateX(0)" },
});

const fadeIn = stylex.keyframes({
  from: { opacity: 0 },
  to: { opacity: 1 },
});

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
    padding: "clamp(18px, 3.6vw, 44px)",
    position: "relative",
  },
  frameBack: (color: string) => ({ backgroundColor: color }),
  dither: {
    height: { default: "100vh", "@supports (height: 100lvh)": "100lvh" },
    insetInline: 0,
    top: 0,
    maskImage: "radial-gradient(ellipse 110% 85% at 50% 0%, black 30%, transparent 92%)",
    pointerEvents: "none",
    position: "absolute",
    zIndex: -1,
  },

  // Prominent top switcher & command bar
  topBar: {
    alignItems: "center",
    display: "flex",
    flexWrap: "wrap",
    gap: 14,
    justifyContent: "space-between",
    marginBottom: 20,
    marginInline: "auto",
    maxWidth: 1240,
  },
  tabTrack: {
    alignItems: "center",
    backdropFilter: "blur(16px) saturate(1.4)",
    backgroundColor: "oklch(0.11 0.02 288 / .84)",
    borderRadius: 14,
    boxShadow: "inset 0 0 0 1px oklch(1 0 0 / .18), 0 12px 32px oklch(0 0 0 / .36)",
    display: { default: "inline-flex", "@media (max-width: 520px)": "grid" },
    gap: 3,
    gridTemplateColumns: {
      default: "none",
      "@media (max-width: 520px)": "repeat(3, minmax(0, 1fr))",
    },
    padding: 5,
    position: "relative",
    width: { default: "auto", "@media (max-width: 520px)": "100%" },
  },
  desktopLabel: {
    "@media (max-width: 520px)": { display: "none" },
  },
  mobileLabel: {
    display: { default: "none", "@media (max-width: 520px)": "inline" },
  },
  tabPill: {
    backgroundColor: "oklch(0.985 0.004 290)",
    borderRadius: 9,
    boxShadow: "0 2px 10px oklch(0 0 0 / .28), inset 0 1px 0 oklch(1 0 0 / .85)",
    left: 0,
    opacity: 0,
    pointerEvents: "none",
    position: "absolute",
    top: 0,
    transition: {
      default:
        "transform 240ms cubic-bezier(0.25, 1, 0.5, 1), width 240ms cubic-bezier(0.25, 1, 0.5, 1), height 240ms cubic-bezier(0.25, 1, 0.5, 1)",
      [REDUCED]: "none",
    },
  },
  tabBtn: {
    alignItems: "center",
    backgroundColor: "transparent",
    borderRadius: 9,
    borderWidth: 0,
    color: "oklch(0.84 0.015 290)",
    cursor: "pointer",
    display: "inline-flex",
    fontFamily: "inherit",
    fontSize: 14.5,
    fontWeight: 600,
    gap: 8,
    justifyContent: "center",
    minHeight: 42,
    paddingInline: 16,
    position: "relative",
    touchAction: "manipulation",
    transition: `color 150ms ease, transform 150ms ${PRESS_EASE}`,
    whiteSpace: "nowrap",
    [HOVER]: { ":hover": { color: "#fff" } },
    ":active": { transform: "scale(.97)" },
    ":focus-visible": { outline: "2px solid oklch(0.97 0 0)", outlineOffset: 2 },
    [MOBILE]: { fontSize: 13.5, gap: 6, minHeight: 40, paddingInline: 10 },
  },
  tabBtnOn: {
    color: "oklch(0.14 0.02 290)",
    [HOVER]: { ":hover": { color: "oklch(0.14 0.02 290)" } },
  },
  tabTrackLight: {
    backgroundColor: "oklch(1 0 0 / .78)",
    boxShadow:
      "inset 0 0 0 1px oklch(0.36 0.08 293 / .16), 0 8px 24px -8px oklch(0.3 0.08 293 / .24)",
  },
  tabPillLight: {
    backgroundColor: siteTokens.text,
    boxShadow: "0 1px 2px oklch(0 0 0 / .16)",
  },
  tabBtnLight: {
    color: siteTokens.muted,
    [HOVER]: { ":hover": { color: siteTokens.text } },
    ":focus-visible": { outline: `2px solid ${siteTokens.text}`, outlineOffset: 2 },
  },
  tabBtnOnLight: {
    color: siteTokens.background,
    [HOVER]: { ":hover": { color: siteTokens.background } },
  },
  barLight: {
    backgroundColor: {
      default: "oklch(1 0 0 / .8)",
      [HOVER]: { ":hover": "oklch(1 0 0 / .94)" },
    },
    borderColor: {
      default: "oklch(0.36 0.08 293 / .18)",
      [HOVER]: { ":hover": "oklch(0.36 0.08 293 / .32)" },
    },
    boxShadow: "0 8px 24px -8px oklch(0.3 0.08 293 / .24)",
    color: siteTokens.text,
    ":focus-visible": { outline: `2px solid ${siteTokens.text}`, outlineOffset: 2 },
  },
  barTextLight: { color: siteTokens.text },
  commandPromptLight: { color: siteTokens.accent },
  topActionMetaLight: {
    backgroundColor: "oklch(0.36 0.08 293 / .08)",
    color: siteTokens.muted,
  },
  copyBadgeLight: {
    backgroundColor: siteTokens.text,
    color: siteTokens.background,
  },
  copyBadgeCopiedLight: { color: "oklch(0.99 0 0)" },
  pointerOnly: { display: { default: "inline-flex", [TOUCH_DEVICE]: "none" } },
  touchOnly: { display: { default: "none", [TOUCH_DEVICE]: "inline-flex" } },

  topActionSlot: {
    alignItems: "center",
    display: "flex",
    maxWidth: "100%",
    "@media (max-width: 520px)": { width: "100%" },
  },
  topActionLink: {
    alignItems: "center",
    backdropFilter: "blur(16px) saturate(1.4)",
    backgroundColor: {
      default: "oklch(0.11 0.02 288 / .84)",
      [HOVER]: { ":hover": "oklch(0.16 0.025 288 / .92)" },
    },
    borderColor: {
      default: "oklch(1 0 0 / .22)",
      [HOVER]: { ":hover": "oklch(1 0 0 / .36)" },
    },
    borderRadius: 12,
    borderStyle: "solid",
    borderWidth: 1,
    boxShadow: "0 12px 30px oklch(0 0 0 / .32)",
    color: "#fff",
    display: "inline-flex",
    fontSize: 13.5,
    fontWeight: 600,
    gap: 9,
    justifyContent: "center",
    minHeight: 46,
    paddingInline: 16,
    textDecoration: "none",
    touchAction: "manipulation",
    transition: `background-color 150ms ease, border-color 150ms ease, transform 150ms ${PRESS_EASE}`,
    ":active": { transform: "scale(.97)" },
    ":focus-visible": { outline: "2px solid oklch(0.97 0 0)", outlineOffset: 2 },
    "@media (max-width: 520px)": { width: "100%" },
  },
  topActionMeta: {
    backgroundColor: "oklch(1 0 0 / .1)",
    borderRadius: 6,
    color: "oklch(0.82 0.015 290)",
    fontSize: 11.5,
    fontWeight: 540,
    padding: "3px 8px",
  },
  commandBar: {
    alignItems: "center",
    backdropFilter: "blur(16px) saturate(1.4)",
    backgroundColor: {
      default: "oklch(0.11 0.02 288 / .88)",
      [HOVER]: { ":hover": "oklch(0.15 0.022 288 / .94)" },
    },
    borderColor: {
      default: "oklch(1 0 0 / .24)",
      [HOVER]: { ":hover": "oklch(1 0 0 / .4)" },
    },
    borderRadius: 12,
    borderStyle: "solid",
    borderWidth: 1,
    boxShadow: "0 12px 30px oklch(0 0 0 / .34)",
    color: "#fff",
    cursor: "pointer",
    display: "inline-flex",
    fontFamily: "inherit",
    gap: 10,
    justifyContent: "space-between",
    maxWidth: "100%",
    minHeight: 46,
    padding: "4px 5px 4px 15px",
    touchAction: "manipulation",
    transition: `background-color 150ms ease, border-color 150ms ease, color 150ms ease, transform 150ms ${PRESS_EASE}`,
    ":active": { transform: "scale(.985)" },
    ":focus-visible": { outline: "2px solid oklch(0.97 0 0)", outlineOffset: 2 },
    "@media (max-width: 520px)": { width: "100%" },
  },
  commandBarCopied: {
    borderColor: {
      default: siteTokens.success,
      [HOVER]: { ":hover": siteTokens.success },
    },
  },
  commandPrompt: {
    color: "oklch(0.82 0.14 293)",
    flexShrink: 0,
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: 14,
    fontWeight: 700,
  },
  commandCode: {
    color: "#fff",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: 13.5,
    fontWeight: 540,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  promptPreview: {
    color: "oklch(0.95 0.005 290)",
    fontSize: 13.5,
    fontWeight: 520,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  copyBadge: {
    alignItems: "center",
    backgroundColor: "oklch(0.985 0.004 290)",
    borderRadius: 8,
    color: "oklch(0.14 0.02 290)",
    display: "inline-flex",
    flexShrink: 0,
    fontSize: 12.5,
    fontWeight: 640,
    gap: 6,
    minHeight: 36,
    paddingInline: 12,
    transition: "background-color 150ms ease, color 150ms ease",
    whiteSpace: "nowrap",
  },
  copyBadgeCopied: {
    backgroundColor: siteTokens.success,
    color: "oklch(0.12 0.02 150)",
  },

  // SmoothHeightStage
  stageOuter: {
    marginInline: "auto",
    maxWidth: 1240,
    overflow: "hidden",
    transition: {
      default: "height 260ms cubic-bezier(0.25, 1, 0.5, 1)",
      [REDUCED]: "none",
    },
  },
  stageAppRadius: { borderRadius: { default: 10, [MOBILE]: "min(3vw, 10px)" } },
  stagePanelRadius: { borderRadius: 14 },
  stageShadowDark: {
    boxShadow:
      "0 0 0 1px oklch(1 0 0 / .1), 0 2px 6px oklch(0 0 0 / .26), 0 32px 84px oklch(0 0 0 / .5)",
  },
  stageShadowLight: {
    boxShadow:
      "0 0 0 1px oklch(0 0 0 / .08), 0 1px 3px oklch(0 0 0 / .06), 0 12px 32px -8px oklch(0 0 0 / .14), 0 32px 64px -24px oklch(0 0 0 / .18)",
  },
  paneForward: {
    animationDuration: { default: "220ms", [REDUCED]: "120ms" },
    animationFillMode: "both",
    animationName: { default: enterFromRight, [REDUCED]: fadeIn },
    animationTimingFunction: "cubic-bezier(0.25, 1, 0.5, 1)",
  },
  paneBackward: {
    animationDuration: { default: "220ms", [REDUCED]: "120ms" },
    animationFillMode: "both",
    animationName: { default: enterFromLeft, [REDUCED]: fadeIn },
    animationTimingFunction: "cubic-bezier(0.25, 1, 0.5, 1)",
  },
  appWindow: {
    boxShadow: "none",
    height: { default: "clamp(600px, calc(100svh - 200px), 780px)", [MOBILE]: 560 },
    marginInline: "auto",
    maxWidth: 1240,
  },

  // Bottom bar inside dithered mat — theme tokens ensure full contrast in both light & dark modes
  bottomBar: {
    alignItems: "center",
    display: "flex",
    flexWrap: "wrap",
    gap: 12,
    justifyContent: "space-between",
    marginInline: "auto",
    marginTop: 18,
    maxWidth: 1240,
    paddingInline: 4,
  },
  bottomSummary: {
    color: siteTokens.muted,
    fontSize: 13.5,
    lineHeight: 1.45,
  },
  bottomDocsLink: {
    alignItems: "center",
    color: siteTokens.text,
    display: "inline-flex",
    fontSize: 13.5,
    fontWeight: 620,
    gap: 6,
    minHeight: 36,
    textDecorationColor: siteTokens.border,
    textDecorationLine: "underline",
    textDecorationThickness: 1,
    textUnderlineOffset: 4,
    transition: "color 140ms ease, text-decoration-color 140ms ease",
    whiteSpace: "nowrap",
    [HOVER]: {
      ":hover": {
        color: siteTokens.accent,
        textDecorationColor: "currentColor",
      },
    },
    ":focus-visible": { outline: `2px solid ${siteTokens.text}`, outlineOffset: 3 },
  },
  visuallyHidden: {
    border: 0,
    clipPath: "inset(50%)",
    height: 1,
    margin: -1,
    overflow: "hidden",
    padding: 0,
    position: "absolute",
    whiteSpace: "nowrap",
    width: 1,
  },
});
