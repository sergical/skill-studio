// PROTO(home-calc): the hero's pointer engine. One passive pointermove stores the position and
// velocity; one requestAnimationFrame loop writes transforms, opacity and two leaf CSS
// variables, and sleeps when everything has settled. No React state changes per move.
// Off for touch and coarse pointers, for reduced motion, and while the hero is off screen.
import { useEffect, type RefObject } from "react";

import { useReducedMotion } from "../../use-reduced-motion";

const FINE_POINTER = "(hover: hover) and (pointer: fine)";
// mass 0.1, stiffness 100, damping 10: Motion's cursor-follower spring.
const MASS = 0.1;
const STIFFNESS = 100;
const DAMPING = 10;
const SUBSTEP = 1 / 240;
// Pointer speed, in px/s, that counts as "fast" for the dither and the stretch.
const FAST = 1800;
const IDLE_MS = 60;

interface HeroPointerTargets {
  root: RefObject<HTMLElement | null>;
  /** Gets --hero-px and --hero-py (−1…1). Keep it a leaf: variables recalc every descendant. */
  vars?: RefObject<HTMLElement | null>;
  /** Springs to the pointer, stretched along its velocity. */
  follower?: RefObject<HTMLElement | null>;
  /** Its opacity rises with pointer speed. */
  speed?: RefObject<HTMLElement | null>;
  /** While hovered, the follower settles on this element instead of the pointer. */
  magnet?: RefObject<HTMLElement | null>;
}

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

export function useHeroPointer({ root, vars, follower, speed, magnet }: HeroPointerTargets) {
  const reduced = useReducedMotion();

  useEffect(() => {
    const hero = root.current;
    if (!hero || reduced || !window.matchMedia(FINE_POINTER).matches) return;

    const pointer = { clientX: 0, clientY: 0, t: 0, vx: 0, vy: 0, inside: false };
    const spring = { x: 0, y: 0, vx: 0, vy: 0, placed: false };
    const lean = { x: 0, y: 0 };
    let fast = 0;
    let presence = 0;
    let magnetOn = false;
    let visible = true;
    let frame = 0;
    let last = 0;

    // The targets are rendered once with the hero, so they are read once here.
    const followerEl = follower?.current ?? null;
    const varsEl = vars?.current ?? null;
    const speedEl = speed?.current ?? null;
    const magnetEl = magnet?.current ?? null;
    const half = followerEl ? followerEl.offsetWidth / 2 : 0;

    const tick = (now: number) => {
      frame = 0;
      // A pointer event inside a frame's rendering steps can stamp `last` after this frame began.
      const dt = Math.max(0, Math.min((now - last) / 1000, 1 / 15));
      last = now;
      const box = hero.getBoundingClientRect();
      const x = pointer.clientX - box.left;
      const y = pointer.clientY - box.top;
      let target = { x, y };
      if (magnetOn && magnetEl) {
        const m = magnetEl.getBoundingClientRect();
        target = { x: m.left - box.left + m.width / 2, y: m.top - box.top + m.height / 2 };
      }

      const moving = now - pointer.t < IDLE_MS;
      const rawSpeed = moving ? Math.hypot(pointer.vx, pointer.vy) : 0;
      fast += (clamp(rawSpeed / FAST, 0, 1) - fast) * (1 - Math.exp(-dt * (rawSpeed ? 10 : 4)));

      let settled = fast < 0.002;

      if (varsEl) {
        const tx = pointer.inside ? (x / box.width) * 2 - 1 : 0;
        const ty = pointer.inside ? (y / box.height) * 2 - 1 : 0;
        const k = 1 - Math.exp(-dt * 6);
        lean.x += (tx - lean.x) * k;
        lean.y += (ty - lean.y) * k;
        varsEl.style.setProperty("--hero-px", lean.x.toFixed(3));
        varsEl.style.setProperty("--hero-py", lean.y.toFixed(3));
        if (Math.abs(tx - lean.x) > 0.001 || Math.abs(ty - lean.y) > 0.001) settled = false;
      }

      if (followerEl) {
        presence += ((pointer.inside ? 1 : 0) - presence) * (1 - Math.exp(-dt * 12));
        if (!pointer.inside && presence < 0.01) {
          presence = 0;
          spring.placed = false;
        }
        if (!spring.placed && !pointer.inside) {
          followerEl.style.opacity = "0";
        } else if (!spring.placed) {
          spring.x = target.x;
          spring.y = target.y;
          spring.placed = true;
        }
        if (spring.placed) {
          const steps = Math.max(1, Math.ceil(dt / SUBSTEP));
          const h = dt / steps;
          for (let i = 0; i < steps; i++) {
            spring.vx += ((-STIFFNESS * (spring.x - target.x) - DAMPING * spring.vx) / MASS) * h;
            spring.vy += ((-STIFFNESS * (spring.y - target.y) - DAMPING * spring.vy) / MASS) * h;
            spring.x += spring.vx * h;
            spring.y += spring.vy * h;
          }
          const angle = Math.atan2(spring.vy, spring.vx);
          const stretch = magnetOn ? 0 : clamp(Math.hypot(spring.vx, spring.vy) / 3000, 0, 0.35);
          const grow = magnetOn ? 1.9 : 1 + fast * 0.25;
          followerEl.style.transform =
            `translate3d(${(spring.x - half).toFixed(1)}px, ${(spring.y - half).toFixed(1)}px, 0) ` +
            `rotate(${angle.toFixed(3)}rad) scale(${(grow * (1 + stretch)).toFixed(3)}, ${(grow * (1 - stretch * 0.4)).toFixed(3)})`;
          const strength = magnetOn ? 0.9 : 0.45 + fast * 0.55;
          followerEl.style.opacity = (presence * strength).toFixed(3);
          if (
            Math.hypot(spring.x - target.x, spring.y - target.y) > 0.2 ||
            Math.hypot(spring.vx, spring.vy) > 2 ||
            Math.abs((pointer.inside ? 1 : 0) - presence) > 0.01
          )
            settled = false;
        }
      }

      if (speedEl) speedEl.style.opacity = (0.72 + fast * 0.28).toFixed(3);

      if (!settled && visible) frame = requestAnimationFrame(tick);
    };

    const wake = () => {
      if (frame || !visible) return;
      last = performance.now();
      frame = requestAnimationFrame(tick);
    };

    const onMove = (event: PointerEvent) => {
      if (event.pointerType !== "mouse" && event.pointerType !== "pen") return;
      const dt = Math.max(event.timeStamp - pointer.t, 1);
      if (pointer.inside && dt < 100) {
        pointer.vx = ((event.clientX - pointer.clientX) / dt) * 1000;
        pointer.vy = ((event.clientY - pointer.clientY) / dt) * 1000;
      } else {
        pointer.vx = 0;
        pointer.vy = 0;
      }
      pointer.clientX = event.clientX;
      pointer.clientY = event.clientY;
      pointer.t = event.timeStamp;
      pointer.inside = true;
      wake();
    };
    const onLeave = () => {
      pointer.inside = false;
      wake();
    };
    const onMagnetEnter = () => {
      magnetOn = true;
      wake();
    };
    const onMagnetLeave = () => {
      magnetOn = false;
      wake();
    };

    hero.addEventListener("pointermove", onMove, { passive: true });
    hero.addEventListener("pointerleave", onLeave, { passive: true });
    magnetEl?.addEventListener("pointerenter", onMagnetEnter, { passive: true });
    magnetEl?.addEventListener("pointerleave", onMagnetLeave, { passive: true });
    const observer = new IntersectionObserver(([entry]) => {
      visible = entry.isIntersecting;
      if (!visible) {
        cancelAnimationFrame(frame);
        frame = 0;
      }
    });
    observer.observe(hero);

    return () => {
      hero.removeEventListener("pointermove", onMove);
      hero.removeEventListener("pointerleave", onLeave);
      magnetEl?.removeEventListener("pointerenter", onMagnetEnter);
      magnetEl?.removeEventListener("pointerleave", onMagnetLeave);
      observer.disconnect();
      cancelAnimationFrame(frame);
      varsEl?.style.removeProperty("--hero-px");
      varsEl?.style.removeProperty("--hero-py");
      if (followerEl) followerEl.style.opacity = "0";
    };
  }, [root, vars, follower, speed, magnet, reduced]);
}
