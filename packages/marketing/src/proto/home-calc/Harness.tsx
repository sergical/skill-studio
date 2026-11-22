// PROTO(home-calc): the picker harness. One variant at a time, full size; the swap has no
// animation and re-mounts the variant so its entrance runs again. Keys: 1–4, ←/→, R to replay.
import {
  memo,
  startTransition,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ComponentType,
} from "react";
import { flushSync } from "react-dom";
import * as stylex from "@stylexjs/stylex";

import { getPaletteTheme } from "../../PaletteThemes.stylex";
import type { ThemeToggleOrigin } from "../../ProductMock";
import {
  applySiteTheme,
  currentSiteTheme,
  onSystemThemeChange,
  rememberSiteTheme,
  rememberedSiteTheme,
  transitionSiteTheme,
} from "../../site-theme";
import type { SiteTheme } from "../../SiteTheme.stylex";
import { CommandCenter } from "../../variants/CommandCenter";
import { BaselinePlus } from "./BaselinePlus";
import { Editorial } from "./Editorial";
import { Kinetic } from "./Kinetic";
import type { VariantProps } from "./ProtoPage";

const VARIANTS: ReadonlyArray<{ name: string; Component: ComponentType<VariantProps> }> = [
  { name: "Current", Component: CommandCenter },
  { name: "Baseline Plus", Component: BaselinePlus },
  { name: "Kinetic", Component: Kinetic },
  { name: "Editorial", Component: Editorial },
];

function initialVariant() {
  const v = Number.parseInt(new URLSearchParams(location.search).get("v") ?? "", 10);
  return v >= 1 && v <= VARIANTS.length ? v - 1 : 0;
}

const Stage = memo(function Stage({
  index,
  mount,
  theme,
  onToggleTheme,
}: {
  index: number;
  mount: number;
  theme: SiteTheme;
  onToggleTheme: (origin: ThemeToggleOrigin) => void;
}) {
  const { Component } = VARIANTS[index];
  return (
    <div {...stylex.props(getPaletteTheme("violet"))}>
      <Component key={`${index}-${mount}`} theme={theme} onToggleTheme={onToggleTheme} />
    </div>
  );
});

export function Harness() {
  // The picker shows the choice at once; the page follows in a transition. A swap re-mounts
  // the whole homepage, and it must not block the key or click that asked for it.
  const [selected, setSelected] = useState(initialVariant);
  const [stage, setStage] = useState(() => ({ index: initialVariant(), mount: 0 }));
  const [ready, setReady] = useState(false);
  const itemRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const highlightRef = useRef<HTMLSpanElement>(null);

  const [theme, setTheme] = useState<SiteTheme>(currentSiteTheme);
  const [isSystemTheme, setIsSystemTheme] = useState(() => rememberedSiteTheme() === null);

  useEffect(() => {
    applySiteTheme(theme);
    if (!isSystemTheme) return;
    return onSystemThemeChange(setTheme);
  }, [isSystemTheme, theme]);

  const toggleTheme = useCallback(
    (origin: ThemeToggleOrigin) => {
      const nextTheme = theme === "dark" ? "light" : "dark";
      rememberSiteTheme(nextTheme);
      transitionSiteTheme(origin, () =>
        flushSync(() => {
          setIsSystemTheme(false);
          setTheme(nextTheme);
          applySiteTheme(nextTheme);
        }),
      );
    },
    [theme],
  );

  const show = (index: number) =>
    startTransition(() => setStage((value) => ({ index, mount: value.mount + 1 })));

  // Pressing the key of the variant on screen is not a switch: only R replays it. Without this,
  // a re-sent key press would re-mount the whole page once per copy.
  const selectedRef = useRef(selected);
  const setActive = (index: number) => {
    if (index < 0 || index >= VARIANTS.length || index === selectedRef.current) return;
    selectedRef.current = index;
    setSelected(index);
    show(index);
    const url = new URL(location.href);
    url.searchParams.set("v", String(index + 1));
    history.replaceState(null, "", url);
  };

  const replay = () => show(selected);

  const moveHighlight = () => {
    const el = itemRefs.current[selected];
    const highlight = highlightRef.current;
    if (!el || !highlight) return;
    highlight.style.width = `${el.offsetWidth}px`;
    highlight.style.transform = `translateX(${el.offsetLeft}px)`;
  };

  useLayoutEffect(moveHighlight);

  useEffect(() => {
    window.addEventListener("resize", moveHighlight);
    return () => window.removeEventListener("resize", moveHighlight);
  });

  useEffect(() => {
    let second = 0;
    const first = requestAnimationFrame(() => {
      second = requestAnimationFrame(() => setReady(true));
    });
    return () => {
      cancelAnimationFrame(first);
      cancelAnimationFrame(second);
    };
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      // A control that handled the key (a slider, a radio group, the jobs tabs) keeps it.
      if (event.defaultPrevented || event.repeat) return;
      const target = event.target;
      if (
        target instanceof HTMLElement &&
        (/^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName) || target.isContentEditable)
      )
        return;
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      const num = Number.parseInt(event.key, 10);
      if (num >= 1 && num <= VARIANTS.length) setActive(num - 1);
      else if (event.key === "ArrowRight") setActive((selected + 1) % VARIANTS.length);
      else if (event.key === "ArrowLeft")
        setActive((selected - 1 + VARIANTS.length) % VARIANTS.length);
      else if (event.key === "r" || event.key === "R") replay();
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  });

  return (
    <>
      <Stage index={stage.index} mount={stage.mount} theme={theme} onToggleTheme={toggleTheme} />
      <nav
        className="proto-picker"
        aria-label="Prototype variants"
        data-ready={ready ? "" : undefined}
      >
        <span ref={highlightRef} className="proto-picker-highlight" aria-hidden="true" />
        {VARIANTS.map((variant, index) => (
          <button
            key={variant.name}
            ref={(el) => {
              itemRefs.current[index] = el;
            }}
            type="button"
            className="proto-picker-item"
            data-active={index === selected ? "" : undefined}
            aria-current={index === selected ? "true" : undefined}
            onClick={() => setActive(index)}
          >
            {variant.name}
          </button>
        ))}
        <span className="proto-picker-divider" aria-hidden="true" />
        <button
          type="button"
          className="proto-picker-item proto-picker-replay"
          aria-label="Replay animation (R)"
          onClick={replay}
        >
          ↻
        </button>
      </nav>
    </>
  );
}
