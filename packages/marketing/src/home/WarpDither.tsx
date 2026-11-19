// ============================================================================
// Skill Studio - Warp dither
// The animated purple dither from the desktop app's welcome screen, drawn at
// one canvas pixel per CSS pixel and 30 frames a second
// ============================================================================

import { useEffect, useLayoutEffect, useRef, useState, type ComponentRef } from "react";
import * as stylex from "@stylexjs/stylex";
import { Dithering } from "@paper-design/shaders-react";

import { useReducedMotion } from "../use-reduced-motion";

const DITHER_SPEED = 0.35;
// The dither moves less than one 3 px cell per 30 fps frame: a 60 fps render of the hero
// shows 0.03% more cell states, each for one frame.
const FRAME_MS = 1000 / 30;
// A frame is due a little early, so vsync jitter on a 60 Hz or 120 Hz display does not
// skip it.
const FRAME_SLACK_MS = 3;
// After a long pause (another tab, a debugger), the animation goes on where it stopped.
const MAX_STEP_MS = 100;

/**
 * Calls `onFrame` with the animation time, 30 times a second on the display's own frames,
 * while `target` is on screen. The time pauses off screen and in a hidden tab. Returns a
 * function that stops it.
 */
export function paceWhileVisible(target: Element, onFrame: (frame: number) => void) {
  let frameId = 0;
  let last = 0;
  let frame = 0;
  const tick = (now: number) => {
    frameId = requestAnimationFrame(tick);
    if (now - last < FRAME_MS - FRAME_SLACK_MS) return;
    frame += Math.min(now - last, MAX_STEP_MS) * DITHER_SPEED;
    last = now;
    onFrame(frame);
  };
  const observer = new IntersectionObserver((entries) => {
    cancelAnimationFrame(frameId);
    // A fast scroll can queue an exit and an entry together; the newest one is current.
    if (!entries[entries.length - 1].isIntersecting) return;
    last = performance.now();
    frameId = requestAnimationFrame(tick);
  });
  observer.observe(target);
  return () => {
    observer.disconnect();
    cancelAnimationFrame(frameId);
  };
}

interface WarpDitherProps {
  colorBack: string;
  colorFront: string;
}

/**
 * Each dither cell is 3 CSS pixels, so a canvas at the screen's pixel ratio only repeats
 * every cell's colour, and a frame every vsync only repeats most cells. This draws a canvas
 * the size of its box in CSS pixels, 30 times a second, while the box is on screen.
 */
export function WarpDither({ colorBack, colorFront }: WarpDitherProps) {
  const boxRef = useRef<HTMLDivElement>(null);
  const shaderRef = useRef<ComponentRef<typeof Dithering>>(null);
  const [cssPixels, setCssPixels] = useState(0);
  const reducedMotion = useReducedMotion();

  useLayoutEffect(() => {
    const box = boxRef.current;
    if (!box) return;
    const observer = new ResizeObserver(([entry]) => {
      const { inlineSize, blockSize } = entry.contentBoxSize[0];
      setCssPixels(Math.round(inlineSize) * Math.round(blockSize));
    });
    observer.observe(box);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const box = boxRef.current;
    if (!box || reducedMotion) return;
    return paceWhileVisible(box, (frame) => shaderRef.current?.paperShaderMount?.setFrame(frame));
  }, [reducedMotion]);

  return (
    <div ref={boxRef} {...stylex.props(styles.box)}>
      {cssPixels > 0 && (
        <Dithering
          ref={shaderRef}
          width="100%"
          height="100%"
          colorBack={colorBack}
          colorFront={colorFront}
          // oxlint-disable-next-line anti-slop/no-shape-in-symbol-names -- Paper Shaders' own prop name
          shape="warp"
          type="4x4"
          size={3}
          scale={0.8}
          speed={0}
          minPixelRatio={1}
          maxPixelCount={cssPixels}
        />
      )}
    </div>
  );
}

const styles = stylex.create({
  // Scales the CSS-pixel canvas up to the screen without blurring the cell edges.
  box: { height: "100%", imageRendering: "pixelated", width: "100%" },
});
