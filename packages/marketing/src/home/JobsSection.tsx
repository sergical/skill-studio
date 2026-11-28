// ============================================================================
// Skill Studio - Interactive Fixer (JobsSection)
// Lets visitors experience five everyday multi-agent skill problems in their
// manual Before state, then click once to transform the stage into Skill
// Studio's live resolution with fluid auto-height and crisp typographic motion.
// ============================================================================
import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import * as stylex from "@stylexjs/stylex";
import { AlertCircle, ArrowRight, CheckCircle2, RotateCcw } from "lucide-react";

import { CLI_DOCS_URL } from "../site-links";
import { siteTokens } from "../SiteTheme.stylex";
import { homeSectionStyles } from "./HomeSection.stylex";
import { SCENARIOS, type Scenario } from "./jobs-scenarios";
import { CopyCliButton, LiveScenarioPreview } from "./JobsLivePreviews";

const HOVER = "@media (hover: hover) and (pointer: fine)";
const REDUCED = "@media (prefers-reduced-motion: reduce)";
const DRAWER_EASE = "cubic-bezier(0.25, 1, 0.5, 1)";
const TYPE_EASE = "cubic-bezier(0.19, 1, 0.22, 1)";
const PRESS_EASE = "cubic-bezier(0.25, 0.46, 0.45, 0.94)";
const CHECK_EASE = "cubic-bezier(0.34, 1.4, 0.64, 1)";
const DIGITS = [0, 1, 2, 3, 4, 5] as const;

type TransitionDirection = 1 | -1;

function prefersReducedMotion() {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** Vertical digit reel for the 0 / 5 fixed counter (tabular-nums, zero width jump). */
function FixedCountDigit({ count }: { count: number }) {
  return (
    <span {...stylex.props(styles.digitMask)} aria-hidden="true">
      <span
        style={{ transform: `translateY(${-count * (100 / DIGITS.length)}%)` }}
        {...stylex.props(styles.digitReel)}
      >
        {DIGITS.map((d) => (
          <span key={d} {...stylex.props(styles.digitCell)}>
            {d}
          </span>
        ))}
      </span>
    </span>
  );
}

function StageViewContent({
  scenario,
  solved,
  onSolve,
}: {
  scenario: Scenario;
  solved: boolean;
  onSolve?: () => void;
}) {
  if (!solved) {
    return (
      <div {...stylex.props(styles.viewBody)}>
        <div {...stylex.props(styles.copyBlock)}>
          <h3 data-anim="headline" {...stylex.props(styles.stageTitle)}>
            {scenario.question}
          </h3>
          <p data-anim="copy" {...stylex.props(styles.stageCopy)}>
            {scenario.painSummary}
          </p>
        </div>

        <pre data-anim="surface" {...stylex.props(styles.terminal)}>
          {scenario.painTerminal.map((line, i) => (
            <span
              key={i}
              {...stylex.props(
                styles.termLine,
                line.tone === "err" && styles.termErr,
                line.tone === "warn" && styles.termWarn,
                line.tone === "dim" && styles.termDim,
              )}
            >
              {line.text}
            </span>
          ))}
        </pre>

        <ul data-anim="detail" {...stylex.props(styles.painList)}>
          {scenario.painSteps.map((step, i) => (
            <li key={i} {...stylex.props(styles.painItem)}>
              <span {...stylex.props(styles.stepDot)} />
              {step}
            </li>
          ))}
        </ul>

        <div data-anim="foot" {...stylex.props(styles.actionBar)}>
          <button
            type="button"
            onClick={onSolve}
            {...stylex.props(stylex.defaultMarker(), styles.solveBtn)}
          >
            <span>{scenario.fixVerb}</span>
            <span aria-hidden="true" {...stylex.props(styles.solveArrow)}>
              <ArrowRight size={15} />
            </span>
          </button>
          <span {...stylex.props(styles.solveHint)}>
            See how Skill Studio handles this in 1 click
          </span>
        </div>
      </div>
    );
  }

  return (
    <div {...stylex.props(styles.viewBody)}>
      <div {...stylex.props(styles.copyBlock)}>
        <h3 data-anim="headline" {...stylex.props(styles.stageTitle)}>
          {scenario.solutionHeadline}
        </h3>
        <p data-anim="copy" {...stylex.props(styles.stageCopy)}>
          {scenario.solutionSummary}
        </p>
      </div>

      <div data-anim="surface">
        <LiveScenarioPreview id={scenario.id} resolved />
      </div>

      <div data-anim="foot" {...stylex.props(styles.solvedFoot)}>
        <CopyCliButton command={scenario.command} />
        <div {...stylex.props(styles.footerLinks)}>
          <a href="#download" {...stylex.props(homeSectionStyles.textLink, styles.ctaLink)}>
            Get the Mac app <span aria-hidden="true">→</span>
          </a>
          <a
            href={CLI_DOCS_URL}
            {...stylex.props(homeSectionStyles.textLink, styles.ctaLink, styles.mutedLink)}
          >
            All CLI commands
          </a>
        </div>
      </div>
    </div>
  );
}

interface TransitionIntent {
  direction: TransitionDirection;
  instant: boolean;
  kind: "state" | "scenario";
}

/**
 * Orchestrates crisp typographic entrance via WAAPI:
 * - Text (`headline`, `copy`, `detail`, `foot`) animates pure opacity + vertical glide
 *   (no scale, no blur — avoids subpixel font-smoothing shimmer).
 * - Only the interactive card (`surface`) gets subtle scale(0.988 -> 1).
 * - Starts at 75ms so the 90ms outgoing fade clears before new letters arrive.
 */
function ActiveStageView({
  scenario,
  solved,
  onSolve,
  intent,
}: {
  scenario: Scenario;
  solved: boolean;
  onSolve: () => void;
  intent: TransitionIntent;
}) {
  const ref = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const root = ref.current;
    if (!root || intent.instant || !("animate" in root)) return;
    const reduced = prefersReducedMotion();
    const anims: Animation[] = [];

    const baseDelay = reduced ? 65 : intent.kind === "state" ? 75 : 55;
    const dir = intent.direction;

    const specs: ReadonlyArray<{
      selector: string;
      delayOffset: number;
      duration: number;
      shift: number;
      scale?: number;
      easing: string;
    }> = [
      {
        selector: '[data-anim="headline"]',
        delayOffset: 0,
        duration: 260,
        shift: intent.kind === "state" ? 10 : 7,
        easing: TYPE_EASE,
      },
      {
        selector: '[data-anim="copy"]',
        delayOffset: 32,
        duration: 240,
        shift: intent.kind === "state" ? 7 : 5,
        easing: TYPE_EASE,
      },
      {
        selector: '[data-anim="surface"]',
        delayOffset: 62,
        duration: 260,
        shift: intent.kind === "state" ? 9 : 6,
        scale: 0.988,
        easing: DRAWER_EASE,
      },
      {
        selector: '[data-anim="detail"]',
        delayOffset: 85,
        duration: 220,
        shift: 5,
        easing: TYPE_EASE,
      },
      {
        selector: '[data-anim="foot"]',
        delayOffset: 95,
        duration: 210,
        shift: 4,
        easing: TYPE_EASE,
      },
    ];

    for (const spec of specs) {
      const el = root.querySelector(spec.selector);
      if (!el || !("animate" in el)) continue;
      const fromTransform = spec.scale
        ? `translateY(${dir * spec.shift}px) scale(${spec.scale})`
        : `translateY(${dir * spec.shift}px)`;
      const toTransform = spec.scale ? "translateY(0px) scale(1)" : "translateY(0px)";

      anims.push(
        el.animate(
          reduced
            ? [{ opacity: 0 }, { opacity: 1 }]
            : [
                { opacity: 0, transform: fromTransform },
                { opacity: 1, transform: toTransform },
              ],
          {
            duration: reduced ? 150 : spec.duration,
            delay: reduced ? baseDelay : baseDelay + spec.delayOffset,
            easing: reduced ? "ease" : spec.easing,
            fill: "both",
          },
        ),
      );
    }

    return () => {
      for (const anim of anims) anim.cancel();
    };
  }, [intent]);

  return (
    <div ref={ref} {...stylex.props(styles.activeLayer)}>
      <StageViewContent scenario={scenario} solved={solved} onSolve={onSolve} />
    </div>
  );
}

interface ExitingSnapshot {
  key: string;
  scenario: Scenario;
  solved: boolean;
  direction: TransitionDirection;
  seq: number;
}

export function JobsSection() {
  const [activeId, setActiveId] = useState<Scenario["id"]>("ignored");
  const [solvedIds, setSolvedIds] = useState<ReadonlyArray<Scenario["id"]>>([]);
  const [railReady, setRailReady] = useState(false);
  const [exiting, setExiting] = useState<ExitingSnapshot | null>(null);
  const [intent, setIntent] = useState<TransitionIntent>({
    direction: 1,
    instant: true,
    kind: "state",
  });

  const outerRef = useRef<HTMLDivElement>(null);
  const measureRef = useRef<HTMLDivElement>(null);
  const exitLayerRef = useRef<HTMLDivElement>(null);
  const prevHeightRef = useRef<number | null>(null);
  const instantHeightRef = useRef(true);
  const seqRef = useRef(0);

  const tabRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const railHighlightRef = useRef<HTMLSpanElement>(null);
  const viaKeyboard = useRef(false);

  const activeIndex = SCENARIOS.findIndex((s) => s.id === activeId);
  const active = SCENARIOS[activeIndex] ?? SCENARIOS[0];
  const isSolved = solvedIds.includes(active.id);
  const viewKey = `${active.id}:${isSolved ? "after" : "before"}`;

  // Fluid adaptive-duration auto-height via ResizeObserver on the padded inner element
  // (Family Drawer pattern: 190–270ms scaled by delta, 0ms on load / keyboard / reduced motion).
  useLayoutEffect(() => {
    const outer = outerRef.current;
    const inner = measureRef.current;
    if (!outer || !inner || !("ResizeObserver" in window)) return;

    const applyHeight = () => {
      const next = Math.ceil(inner.getBoundingClientRect().height);
      if (next <= 0) return;
      const prev = prevHeightRef.current;
      prevHeightRef.current = next;

      if (prev === null || instantHeightRef.current || prefersReducedMotion()) {
        outer.style.transitionDuration = "0ms";
        outer.style.height = `${next}px`;
        return;
      }

      const delta = Math.abs(next - prev);
      const adaptive = Math.round(Math.min(Math.max((delta / 500) * 1000, 190), 270));
      outer.style.transitionDuration = `${adaptive}ms`;
      outer.style.height = `${next}px`;
    };

    applyHeight();
    const observer = new ResizeObserver(applyHeight);
    observer.observe(inner);
    return () => observer.disconnect();
  }, []);

  // Fast 90ms outgoing fade (pure opacity + 4px drift, no blur/scale) so text never collides.
  useLayoutEffect(() => {
    const el = exitLayerRef.current;
    if (!exiting || !el || !("animate" in el)) return;
    const reduced = prefersReducedMotion();
    const anim = el.animate(
      reduced
        ? [{ opacity: 1 }, { opacity: 0 }]
        : [
            { opacity: 1, transform: "translateY(0px)" },
            { opacity: 0, transform: `translateY(${-exiting.direction * 4}px)` },
          ],
      {
        duration: 90,
        easing: "cubic-bezier(0.23, 1, 0.32, 1)",
        fill: "forwards",
      },
    );
    let cancelled = false;
    void anim.finished
      .then(() => {
        if (!cancelled) setExiting((curr) => (curr?.seq === exiting.seq ? null : curr));
      })
      .catch(() => {
        // Cancelled mid-flight by rapid click; next transition takes over cleanly.
      });
    return () => {
      cancelled = true;
      anim.cancel();
    };
  }, [exiting]);

  // Sliding left-rail highlight (smooth on pointer click, instant 0ms on keyboard arrow nav).
  useLayoutEffect(() => {
    const syncHighlight = () => {
      const el = tabRefs.current[activeIndex];
      const highlight = railHighlightRef.current;
      if (!el || !highlight) return;
      if (viaKeyboard.current || prefersReducedMotion()) {
        highlight.style.transition = "none";
      } else {
        highlight.style.transition = `transform 240ms ${DRAWER_EASE}, height 240ms ${DRAWER_EASE}`;
      }
      viaKeyboard.current = false;
      highlight.style.height = `${el.offsetHeight}px`;
      highlight.style.transform = `translateY(${el.offsetTop}px)`;
    };
    syncHighlight();
    window.addEventListener("resize", syncHighlight);
    return () => window.removeEventListener("resize", syncHighlight);
  }, [activeIndex]);

  useEffect(() => {
    let second = 0;
    const first = requestAnimationFrame(() => {
      second = requestAnimationFrame(() => setRailReady(true));
    });
    return () => {
      cancelAnimationFrame(first);
      cancelAnimationFrame(second);
    };
  }, []);

  const snapshotExit = (direction: TransitionDirection) => {
    seqRef.current += 1;
    setExiting({
      key: viewKey,
      scenario: active,
      solved: isSolved,
      direction,
      seq: seqRef.current,
    });
  };

  const solveActive = () => {
    if (isSolved) return;
    instantHeightRef.current = false;
    setIntent({ direction: 1, instant: false, kind: "state" });
    snapshotExit(1);
    setSolvedIds((prev) => (prev.includes(active.id) ? prev : [...prev, active.id]));
  };

  const resetActive = () => {
    if (!isSolved) return;
    instantHeightRef.current = false;
    setIntent({ direction: -1, instant: false, kind: "state" });
    snapshotExit(-1);
    setSolvedIds((prev) => prev.filter((item) => item !== active.id));
  };

  const pickScenario = (nextIndex: number, keyboard: boolean) => {
    const next = SCENARIOS[nextIndex];
    if (!next || next.id === active.id) return;
    const direction: TransitionDirection = nextIndex > activeIndex ? 1 : -1;
    viaKeyboard.current = keyboard;
    instantHeightRef.current = keyboard;
    setIntent({ direction, instant: keyboard, kind: "scenario" });
    if (keyboard) setExiting(null);
    else snapshotExit(direction);
    setActiveId(next.id);
  };

  const onTabKey = (event: KeyboardEvent) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    event.preventDefault();
    const delta = event.key === "ArrowDown" ? 1 : -1;
    const nextIndex = (activeIndex + delta + SCENARIOS.length) % SCENARIOS.length;
    pickScenario(nextIndex, true);
    document.getElementById(`fixer-tab-${SCENARIOS[nextIndex].id}`)?.focus();
  };

  return (
    <>
      <div {...stylex.props(styles.headerRow)}>
        <div>
          <h2 {...stylex.props(homeSectionStyles.title, styles.title)}>
            Stop hunting through skill folders.
          </h2>
          <p {...stylex.props(styles.lede)}>
            Pick a problem you hit today, see what it takes by hand, then click to run Skill Studio
            on it.
          </p>
        </div>

        <span
          aria-live="polite"
          {...stylex.props(styles.progressPill, solvedIds.length > 0 && styles.progressPillOn)}
        >
          <span
            {...stylex.props(
              styles.progressDot,
              solvedIds.length === SCENARIOS.length && styles.progressDotDone,
            )}
          />
          <span {...stylex.props(styles.visuallyHidden)}>
            {solvedIds.length} of {SCENARIOS.length} fixed
          </span>
          <span aria-hidden="true" {...stylex.props(styles.progressCount)}>
            <FixedCountDigit count={solvedIds.length} />
            <span>/ {SCENARIOS.length} fixed</span>
          </span>
        </span>
      </div>

      <div {...stylex.props(styles.split)}>
        <div
          role="tablist"
          aria-label="Skill problems to fix"
          onKeyDown={onTabKey}
          {...stylex.props(styles.rail)}
        >
          <span
            ref={railHighlightRef}
            aria-hidden="true"
            {...stylex.props(styles.railHighlight, !railReady && styles.noTransition)}
          />
          {SCENARIOS.map((item, i) => {
            const on = item.id === active.id;
            const solved = solvedIds.includes(item.id);
            return (
              <button
                key={item.id}
                ref={(el) => {
                  tabRefs.current[i] = el;
                }}
                id={`fixer-tab-${item.id}`}
                type="button"
                role="tab"
                aria-selected={on}
                aria-controls="fixer-panel"
                tabIndex={on ? 0 : -1}
                onClick={() => pickScenario(i, false)}
                {...stylex.props(styles.tab, on && styles.tabOn)}
              >
                <span aria-hidden="true" {...stylex.props(styles.iconSlot)}>
                  <span
                    {...stylex.props(
                      styles.iconFace,
                      styles.iconWarn,
                      !solved && styles.iconFaceOn,
                    )}
                  >
                    <AlertCircle size={15} />
                  </span>
                  <span
                    {...stylex.props(styles.iconFace, styles.iconGood, solved && styles.iconFaceOn)}
                  >
                    <CheckCircle2 size={15} />
                  </span>
                </span>
                <span {...stylex.props(styles.tabCopy)}>
                  <span {...stylex.props(styles.tabTitle)}>{item.question}</span>
                  <span {...stylex.props(styles.metaMask)}>
                    <span
                      {...stylex.props(
                        styles.metaFace,
                        !solved ? styles.metaFaceOn : styles.metaFaceOutUp,
                      )}
                    >
                      {item.painCost}
                    </span>
                    <span
                      {...stylex.props(
                        styles.metaFace,
                        styles.metaGood,
                        solved ? styles.metaFaceOn : styles.metaFaceOutDown,
                      )}
                    >
                      Solved · {item.solutionOutcome}
                    </span>
                  </span>
                </span>
              </button>
            );
          })}
        </div>

        <div
          ref={outerRef}
          id="fixer-panel"
          role="tabpanel"
          aria-labelledby={`fixer-tab-${active.id}`}
          {...stylex.props(styles.stageOuter)}
        >
          <span
            aria-hidden="true"
            {...stylex.props(styles.solvedRing, isSolved && styles.solvedRingOn)}
          />

          <div ref={measureRef} {...stylex.props(styles.stageInner)}>
            <div {...stylex.props(styles.stageTop)}>
              <div {...stylex.props(styles.badgeSlot)}>
                <span
                  aria-hidden={isSolved}
                  {...stylex.props(
                    styles.modeBadge,
                    styles.modePain,
                    styles.badgeFace,
                    !isSolved ? styles.badgeFaceOn : styles.badgeFaceOutUp,
                  )}
                >
                  Today, by hand · {active.painCost}
                </span>
                <span
                  aria-hidden={!isSolved}
                  {...stylex.props(
                    styles.modeBadge,
                    styles.modeGood,
                    styles.badgeFace,
                    isSolved ? styles.badgeFaceOn : styles.badgeFaceOutDown,
                  )}
                >
                  With Skill Studio · {active.solutionOutcome}
                </span>
              </div>

              <button
                type="button"
                onClick={resetActive}
                aria-hidden={!isSolved}
                tabIndex={isSolved ? 0 : -1}
                {...stylex.props(styles.resetBtn, isSolved && styles.resetBtnOn)}
              >
                <RotateCcw size={12} aria-hidden="true" />
                Compare with before
              </button>
            </div>

            <div {...stylex.props(styles.viewViewport)}>
              {exiting && (
                <div
                  key={`exit-${exiting.seq}`}
                  ref={exitLayerRef}
                  aria-hidden="true"
                  inert
                  {...stylex.props(styles.exitLayer)}
                >
                  <StageViewContent scenario={exiting.scenario} solved={exiting.solved} />
                </div>
              )}
              <ActiveStageView
                key={viewKey}
                scenario={active}
                solved={isSolved}
                onSolve={solveActive}
                intent={intent}
              />
            </div>
          </div>
        </div>
      </div>
    </>
  );
}

const styles = stylex.create({
  visuallyHidden: {
    clip: "rect(0 0 0 0)",
    clipPath: "inset(50%)",
    height: 1,
    overflow: "hidden",
    position: "absolute",
    whiteSpace: "nowrap",
    width: 1,
  },
  headerRow: {
    alignItems: "flex-end",
    display: "flex",
    flexWrap: "wrap",
    gap: "12px 24px",
    justifyContent: "space-between",
    marginBottom: 32,
    "@media (max-width: 760px)": { marginBottom: 24 },
  },
  title: { marginBottom: 14 },
  lede: {
    color: siteTokens.muted,
    fontSize: 17,
    lineHeight: 1.6,
    margin: 0,
    maxWidth: "58ch",
    textWrap: "pretty",
    "@media (max-width: 760px)": { fontSize: 16 },
  },
  progressPill: {
    alignItems: "center",
    backgroundColor: siteTokens.surface,
    borderColor: siteTokens.border,
    borderRadius: 999,
    borderStyle: "solid",
    borderWidth: 1,
    color: siteTokens.muted,
    display: "inline-flex",
    fontSize: 12.5,
    fontWeight: 580,
    gap: 8,
    height: 32,
    paddingInline: 12,
    transition: "border-color 220ms ease, color 220ms ease",
  },
  progressPillOn: { color: siteTokens.text },
  progressDot: {
    backgroundColor: siteTokens.warning,
    borderRadius: 999,
    height: 7,
    transition: "background-color 220ms ease",
    width: 7,
  },
  progressDotDone: { backgroundColor: siteTokens.success },
  progressCount: {
    alignItems: "center",
    display: "inline-flex",
    fontVariantNumeric: "tabular-nums",
    gap: 3,
  },
  digitMask: {
    display: "inline-block",
    height: "1.2em",
    lineHeight: 1.2,
    overflow: "hidden",
  },
  digitReel: {
    display: "flex",
    flexDirection: "column",
    transitionDuration: { default: "240ms", [REDUCED]: "0ms" },
    transitionProperty: "transform",
    transitionTimingFunction: DRAWER_EASE,
  },
  digitCell: {
    display: "block",
    height: "1.2em",
    lineHeight: 1.2,
    textAlign: "center",
  },
  split: {
    alignItems: "start",
    display: "grid",
    gap: 20,
    gridTemplateColumns: "360px minmax(0, 1fr)",
    "@media (max-width: 960px)": { gridTemplateColumns: "minmax(0, 1fr)" },
  },
  rail: {
    display: "flex",
    flexDirection: "column",
    gap: 6,
    position: "relative",
  },
  railHighlight: {
    backgroundColor: siteTokens.surface,
    borderColor: siteTokens.border,
    borderRadius: 12,
    borderStyle: "solid",
    borderWidth: 1,
    left: 0,
    pointerEvents: "none",
    position: "absolute",
    right: 0,
    top: 0,
  },
  noTransition: { transition: "none" },
  tab: {
    alignItems: "flex-start",
    backgroundColor: "transparent",
    borderColor: "transparent",
    borderRadius: 12,
    borderStyle: "solid",
    borderWidth: 1,
    color: siteTokens.muted,
    cursor: "pointer",
    display: "flex",
    fontFamily: "inherit",
    gap: 10,
    minHeight: 62,
    padding: "12px 14px",
    position: "relative",
    textAlign: "left",
    transition: `color 150ms ease, transform 150ms ${PRESS_EASE}`,
    [HOVER]: { ":hover": { color: siteTokens.text } },
    ":active": { transform: "scale(.985)" },
    ":focus-visible": { outline: `2px solid ${siteTokens.muted}`, outlineOffset: 2 },
  },
  tabOn: { color: siteTokens.text },
  iconSlot: {
    display: "inline-grid",
    flexShrink: 0,
    height: 16,
    marginTop: 2,
    placeItems: "center",
    width: 16,
  },
  iconFace: {
    display: "inline-flex",
    gridArea: "1 / 1",
    opacity: 0,
    transform: "scale(0.7) rotate(-14deg)",
    transitionDuration: "240ms",
    transitionProperty: "opacity, transform",
    transitionTimingFunction: CHECK_EASE,
    [REDUCED]: { transform: "none", transitionProperty: "opacity" },
  },
  iconFaceOn: {
    opacity: 1,
    transform: "scale(1) rotate(0deg)",
  },
  iconWarn: { color: siteTokens.warning },
  iconGood: { color: siteTokens.success },
  tabCopy: { display: "flex", flexDirection: "column", gap: 3, minWidth: 0 },
  tabTitle: {
    color: "inherit",
    fontSize: 14.5,
    fontWeight: 620,
    letterSpacing: "-.01em",
    lineHeight: 1.3,
  },
  metaMask: {
    display: "inline-grid",
    lineHeight: 1.35,
    minWidth: 0,
    overflow: "hidden",
  },
  metaFace: {
    color: siteTokens.muted,
    fontSize: 12,
    fontVariantNumeric: "tabular-nums",
    gridArea: "1 / 1",
    transitionProperty: "opacity, transform",
    transitionTimingFunction: TYPE_EASE,
    [REDUCED]: { transform: "none", transitionProperty: "opacity" },
  },
  metaFaceOn: {
    opacity: 1,
    transform: "translateY(0%)",
    transitionDelay: "65ms",
    transitionDuration: "220ms",
  },
  metaFaceOutUp: {
    opacity: 0,
    transform: { default: "translateY(-85%)", [REDUCED]: "none" },
    transitionDelay: "0ms",
    transitionDuration: "90ms",
  },
  metaFaceOutDown: {
    opacity: 0,
    transform: { default: "translateY(85%)", [REDUCED]: "none" },
    transitionDelay: "0ms",
    transitionDuration: "90ms",
  },
  metaGood: { color: siteTokens.success },
  stageOuter: {
    backgroundColor: siteTokens.surface,
    borderColor: siteTokens.border,
    borderRadius: 16,
    borderStyle: "dashed",
    borderWidth: 1,
    isolation: "isolate",
    minWidth: 0,
    overflow: "hidden",
    position: "relative",
    transitionProperty: "height",
    transitionTimingFunction: DRAWER_EASE,
    [REDUCED]: { transitionProperty: "none" },
  },
  solvedRing: {
    borderColor: `color-mix(in oklch, ${siteTokens.success} 45%, ${siteTokens.border})`,
    borderRadius: 16,
    borderStyle: "solid",
    borderWidth: 1,
    boxShadow: `inset 0 0 0 1px color-mix(in oklch, ${siteTokens.success} 14%, transparent)`,
    inset: 0,
    opacity: 0,
    pointerEvents: "none",
    position: "absolute",
    transitionDuration: "240ms",
    transitionProperty: "opacity",
    transitionTimingFunction: DRAWER_EASE,
    zIndex: 2,
  },
  solvedRingOn: { opacity: 1 },
  stageInner: {
    display: "flex",
    flexDirection: "column",
    gap: 18,
    padding: 24,
    "@media (max-width: 600px)": { padding: 18 },
  },
  stageTop: {
    alignItems: "center",
    display: "flex",
    flexWrap: "wrap",
    gap: 10,
    justifyContent: "space-between",
    minHeight: 32,
  },
  badgeSlot: { display: "inline-grid", alignItems: "center" },
  modeBadge: {
    borderRadius: 999,
    fontSize: 12,
    fontWeight: 620,
    gridArea: "1 / 1",
    justifySelf: "start",
    padding: "4px 10px",
    whiteSpace: "nowrap",
  },
  badgeFace: {
    transitionProperty: "opacity, transform",
    transitionTimingFunction: TYPE_EASE,
    [REDUCED]: { transitionProperty: "opacity" },
  },
  badgeFaceOn: {
    opacity: 1,
    transform: "translateY(0)",
    transitionDelay: "65ms",
    transitionDuration: "220ms",
  },
  badgeFaceOutUp: {
    opacity: 0,
    pointerEvents: "none",
    transform: { default: "translateY(-5px)", [REDUCED]: "none" },
    transitionDelay: "0ms",
    transitionDuration: "90ms",
  },
  badgeFaceOutDown: {
    opacity: 0,
    pointerEvents: "none",
    transform: { default: "translateY(5px)", [REDUCED]: "none" },
    transitionDelay: "0ms",
    transitionDuration: "90ms",
  },
  modePain: {
    backgroundColor: siteTokens.background,
    boxShadow: `inset 0 0 0 1px ${siteTokens.border}`,
    color: siteTokens.warning,
  },
  modeGood: {
    backgroundColor: siteTokens.background,
    boxShadow: `inset 0 0 0 1px color-mix(in oklch, ${siteTokens.success} 45%, transparent)`,
    color: siteTokens.success,
  },
  resetBtn: {
    alignItems: "center",
    backgroundColor: "transparent",
    borderColor: { default: siteTokens.border, [HOVER]: { ":hover": siteTokens.muted } },
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    color: siteTokens.muted,
    cursor: "pointer",
    display: "inline-flex",
    fontFamily: "inherit",
    fontSize: 12,
    fontWeight: 560,
    gap: 6,
    minHeight: 32,
    opacity: 0,
    paddingInline: 10,
    pointerEvents: "none",
    position: "relative",
    transform: { default: "translateY(4px)", [REDUCED]: "none" },
    transitionDelay: "0ms",
    transitionDuration: "90ms",
    transitionProperty: "opacity, transform, color, border-color",
    transitionTimingFunction: TYPE_EASE,
    "::before": { content: '""', inset: "-6px -4px", position: "absolute" },
    [HOVER]: { ":hover": { color: siteTokens.text } },
    ":active": { transform: "scale(.97)" },
    ":focus-visible": { outline: `2px solid ${siteTokens.muted}`, outlineOffset: 2 },
  },
  resetBtnOn: {
    opacity: 1,
    pointerEvents: "auto",
    transform: "translateY(0)",
    transitionDelay: "65ms",
    transitionDuration: "220ms",
  },
  viewViewport: {
    minWidth: 0,
    position: "relative",
  },
  exitLayer: {
    left: 0,
    pointerEvents: "none",
    position: "absolute",
    right: 0,
    top: 0,
  },
  activeLayer: {
    minWidth: 0,
  },
  viewBody: {
    display: "flex",
    flexDirection: "column",
    gap: 18,
    minWidth: 0,
  },
  copyBlock: { display: "flex", flexDirection: "column", gap: 6 },
  stageTitle: {
    color: siteTokens.text,
    fontSize: 22,
    fontWeight: 660,
    letterSpacing: "-.025em",
    lineHeight: 1.25,
    margin: 0,
    textWrap: "balance",
  },
  stageCopy: {
    color: siteTokens.muted,
    fontSize: 15,
    lineHeight: 1.55,
    margin: 0,
    maxWidth: "56ch",
    textWrap: "pretty",
  },
  terminal: {
    backgroundColor: siteTokens.background,
    borderColor: siteTokens.border,
    borderRadius: 11,
    borderStyle: "solid",
    borderWidth: 1,
    color: siteTokens.text,
    display: "flex",
    flexDirection: "column",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: 12,
    gap: 5,
    lineHeight: 1.55,
    margin: 0,
    padding: "13px 14px",
  },
  termLine: { overflowWrap: "anywhere", whiteSpace: "pre-wrap" },
  termErr: { color: siteTokens.danger },
  termWarn: { color: siteTokens.warning },
  termDim: { color: siteTokens.muted },
  painList: {
    color: siteTokens.muted,
    display: "grid",
    fontSize: 13.5,
    gap: "8px 18px",
    gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
    lineHeight: 1.45,
    listStyle: "none",
    margin: 0,
    padding: 0,
    "@media (max-width: 680px)": { gridTemplateColumns: "minmax(0, 1fr)" },
  },
  painItem: { alignItems: "flex-start", display: "flex", gap: 8 },
  stepDot: {
    backgroundColor: siteTokens.border,
    borderRadius: 999,
    flexShrink: 0,
    height: 6,
    marginTop: 7,
    width: 6,
  },
  actionBar: {
    alignItems: "center",
    borderTopColor: siteTokens.border,
    borderTopStyle: "solid",
    borderTopWidth: 1,
    display: "flex",
    flexWrap: "wrap",
    gap: "10px 16px",
    paddingTop: 16,
  },
  solveBtn: {
    alignItems: "center",
    backgroundColor: { default: siteTokens.text, [HOVER]: { ":hover": siteTokens.accent } },
    borderRadius: 10,
    borderWidth: 0,
    color: siteTokens.background,
    cursor: "pointer",
    display: "inline-flex",
    fontFamily: "inherit",
    fontSize: 14,
    fontWeight: 640,
    gap: 8,
    minHeight: 44,
    // Optical padding: icon side is 2px tighter than text side (emil-ui-polish #9).
    padding: "0 14px 0 16px",
    transition: `background-color 150ms ease, transform 150ms ${PRESS_EASE}`,
    ":active": { transform: "scale(.97)" },
    ":focus-visible": { outline: `2px solid ${siteTokens.muted}`, outlineOffset: 2 },
  },
  solveArrow: {
    display: "inline-flex",
    transform: {
      default: "translateX(0)",
      [HOVER]: {
        [stylex.when.ancestor(":hover")]: "translateX(3px)",
      },
    },
    transition: {
      default: `transform 180ms ${DRAWER_EASE}`,
      [REDUCED]: "none",
    },
  },
  solveHint: { color: siteTokens.muted, fontSize: 13 },
  solvedFoot: {
    alignItems: "center",
    borderTopColor: siteTokens.border,
    borderTopStyle: "solid",
    borderTopWidth: 1,
    display: "flex",
    flexWrap: "wrap",
    gap: "10px 16px",
    justifyContent: "space-between",
    paddingTop: 14,
  },
  footerLinks: { alignItems: "center", display: "flex", flexWrap: "wrap", gap: "4px 18px" },
  ctaLink: { minHeight: 40 },
  mutedLink: { color: siteTokens.muted },
});
