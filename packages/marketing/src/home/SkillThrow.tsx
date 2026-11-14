// ============================================================================
// Skill Studio - Skill throw
// The homepage hero pile is physical. Grab a skill and throw it: it tumbles,
// knocks its neighbours and lands on the heap. Throw it at the mascot and the
// mascot sorts it. A click pops a copy into the parked drawer. A click on the
// mascot tidies everything. One rAF loop runs damped springs and sleeps when
// nothing moves.
// ============================================================================
import { useEffect, useEffectEvent, useLayoutEffect, useRef, useState } from "react";

import { useReducedMotion } from "../use-reduced-motion";
import { PILE, PILE_COUNTS, STATE_COLOR } from "./skill-pile-data";
import {
  KEPT_SLOT,
  PileChipView,
  PileDrawer,
  PileFrame,
  PileMascot,
  fly,
  frame,
  geometry,
  heapPoses,
  introSeen,
  lerpPose,
  markIntroSeen,
  place,
  poseTransform,
  pulse,
  rand,
  setCount,
  setStyle,
  settleAnimations,
  slotPose,
  squash,
  usePileStage,
  type Geometry,
  type Point,
  type Pose,
} from "./skill-pile-kit";

const INTRO = "throw";
const DRAG_THRESHOLD = 5;
const REPEL_RADIUS = 90;
const SUBSTEP = 1 / 240;
const RAD = Math.PI / 180;
const GRAVITY = 2400;
const AIR_DRAG = 0.35;
const SPIN_DRAG = 0.8;
const MAX_SPEED = 3200;
const MAX_RISE = 2000;
const SAMPLE_WINDOW_MS = 100;
const STALE_FROM_MS = 30;
const STALE_UNTIL_MS = 100;
const KICK_SPEED = 240;
const KNOCK_LOOSE = 600;
const MAX_KNOCKS = 3;
const RESTITUTION = 0.38;
const BOUNCE_MIN = 320;
const TUMBLE = 0.3;
const FRICTION = 1500;
const MAGNET = 5200;
// A held chip hangs from the grab point: the pointer is the pivot, gravity and drag swing it,
// and a torque spring stands it upright so it can be read.
const PIVOT_HZ = 30;
const HELD_GRAVITY = 1200;
const UPRIGHT = 277_000;
const SWING_ZETA = 0.45;
const INERTIAL_COUPLING = 0.08;
const HELD_AIR = 1.6;
const MAX_SWING = 12;

type Mode = "rest" | "held" | "flying" | "gone";

interface Body {
  mode: Mode;
  awake: boolean;
  grounded: boolean;
  x: number;
  y: number;
  vx: number;
  vy: number;
  r: number;
  vr: number;
  s: number;
  vs: number;
  q: number;
  vq: number;
  home: { x: number; y: number; r: number };
  push: Point;
  half: number;
  halfH: number;
  k2: number;
  kickedAt: number;
  knocks: number;
}

interface Hold {
  index: number;
  pointerId: number;
  start: Point;
  local: Point;
  upright: number;
  target: Point;
  pivot: Point;
  pivotV: Point;
  moved: boolean;
  samples: Array<{ t: number; x: number; y: number }>;
}

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));
const wrapAngle = (degrees: number) => ((((degrees + 180) % 360) + 360) % 360) - 180;
const jitter = (scale: number) => (Math.random() - 0.5) * scale;
const clock = () => performance.now();
const poppable = (index: number) => !KEPT_SLOT.has(PILE[index].id);

// Damped spring, integrated in 240 Hz substeps so a slow frame cannot blow it up.
function spring(x: number, v: number, target: number, hz: number, zeta: number, dt: number) {
  const w = 2 * Math.PI * hz;
  const steps = Math.max(1, Math.ceil(dt / SUBSTEP));
  const h = dt / steps;
  for (let i = 0; i < steps; i++) {
    v += (-w * w * (x - target) - 2 * zeta * w * v) * h;
    x += v * h;
  }
  return [x, v] as const;
}

// Vector from the grab point to the chip centre, in stage space.
function armOf(body: Body, local: Point): Point {
  const cos = Math.cos(body.r * RAD);
  const sin = Math.sin(body.r * RAD);
  return {
    x: body.s * (cos * local.x - sin * local.y),
    y: body.s * (sin * local.x + cos * local.y),
  };
}

function releaseVelocity(samples: Hold["samples"], now: number): Point {
  const last = samples[samples.length - 1];
  const recent = samples.filter((sample) => last.t - sample.t <= SAMPLE_WINDOW_MS);
  if (recent.length < 2) return { x: 0, y: 0 };
  const first = recent[0];
  const elapsed = Math.max(last.t - first.t, 8) / 1000;
  const fresh = clamp(1 - (now - last.t - STALE_FROM_MS) / (STALE_UNTIL_MS - STALE_FROM_MS), 0, 1);
  return {
    x: ((last.x - first.x) / elapsed) * fresh,
    y: ((last.y - first.y) / elapsed) * fresh,
  };
}

const parkedPose = (g: Geometry): Pose => ({ ...g.drawer, r: 0, s: 0.5, o: 0 });
const targetPose = (g: Geometry, id: string) => {
  const slot = KEPT_SLOT.get(id);
  return slot === undefined ? parkedPose(g) : slotPose(g, slot);
};

export function SkillThrow() {
  const reduced = useReducedMotion();
  const { stageRef, chipRefs, mascotRef, drawerRef, countRef, size } = usePileStage();
  const fxRef = useRef<HTMLDivElement>(null);
  const [collectedCount, setCollectedCount] = useState(0);
  const [ready, setReady] = useState(false);
  const bodies = useRef<Body[]>([]);
  const mascot = useRef({ s: 1, vs: 0, r: 0, vr: 0, lean: 0, open: false, awake: false });
  const geo = useRef<Geometry | null>(null);
  const collected = useRef(new Set<number>());
  const parked = useRef(0);
  const hold = useRef<Hold | null>(null);
  const frameId = useRef(0);
  const lastTime = useRef(0);
  const slowUntil = useRef(0);
  const started = useRef(false);
  const g = size ? geometry(size.w, size.h) : null;
  const tidy = collectedCount === PILE.length;

  useEffect(() => () => cancelAnimationFrame(frameId.current), []);

  const markCollected = (index: number) => {
    collected.current.add(index);
    setCollectedCount(collected.current.size);
  };

  const countParked = () => {
    parked.current += 1;
    setCount(countRef, parked.current);
    pulse(drawerRef.current, 1.06);
  };

  const wakeBody = (index: number) => {
    const body = bodies.current[index];
    if (!body || body.awake) return;
    const element = chipRefs.current[index];
    if (element) settleAnimations(element);
    body.awake = true;
  };

  const wakeMascot = () => {
    const state = mascot.current;
    if (state.awake) return;
    if (mascotRef.current) settleAnimations(mascotRef.current);
    state.awake = true;
  };

  const hitStop = (ms: number) => {
    slowUntil.current = clock() + ms;
  };

  // Sends a chip to its kept slot or the parked drawer. With `via`, it goes through the mascot.
  // With `pop`, it swells in place first.
  const sendAway = (
    index: number,
    { delay = 0, via, pop = false }: { delay?: number; via?: Point; pop?: boolean } = {},
  ) => {
    const g = geo.current;
    const body = bodies.current[index];
    if (!g || !body || collected.current.has(index)) return;
    body.mode = "gone";
    body.awake = false;
    markCollected(index);
    const element = chipRefs.current[index];
    const chip = PILE[index];
    const keep = KEPT_SLOT.has(chip.id);
    const from: Pose = { x: body.x, y: body.y, r: body.r, s: body.s };
    const to = targetPose(g, chip.id);
    setStyle(element, { zIndex: keep || pop ? "2" : "" });
    const arc = frame(
      { ...lerpPose(from, to, 0.5), y: Math.min(from.y, to.y) - (keep ? 40 : 70), r: 0 },
      { offset: pop ? 0.55 : 0.45 },
    );
    const frames = via
      ? [frame({ ...via, r: 0, s: 0.5 }, { offset: 0.35 }), frame(to)]
      : pop
        ? [frame({ ...from, y: from.y - 12, s: body.s * 1.22 }, { offset: 0.18 }), arc, frame(to)]
        : [arc, frame(to)];
    fly(
      element,
      frames,
      { duration: via ? 560 : pop ? 620 : keep ? 640 : 540, delay },
      keep ? undefined : countParked,
    );
  };

  const catchChip = (index: number) => {
    const g = geo.current;
    const body = bodies.current[index];
    if (!g || !body) return;
    hitStop(40);
    wakeMascot();
    mascot.current.vs += 3.2;
    mascot.current.vr += clamp(body.vx * 0.08, -160, 160);
    sendAway(index, { via: g.mascot });
    tick();
  };

  const burst = (at: Point, color: string) => {
    const layer = fxRef.current;
    if (!layer) return;
    const ring = document.createElement("span");
    Object.assign(ring.style, {
      border: `2px solid ${color}`,
      borderRadius: "50%",
      height: "64px",
      left: `${at.x - 32}px`,
      position: "absolute",
      top: `${at.y - 32}px`,
      width: "64px",
    });
    layer.append(ring);
    ring
      .animate(
        [
          { scale: "0.25", opacity: 0.9 },
          { scale: "1.6", opacity: 0 },
        ],
        {
          duration: 320,
          easing: "cubic-bezier(0.2, 0.8, 0.2, 1)",
        },
      )
      .finished.finally(() => ring.remove());
  };

  const shockwave = (at: Point, self: number) => {
    bodies.current.forEach((body, index) => {
      if (index === self || body.mode !== "rest") return;
      const dx = body.x - at.x;
      const dy = body.y - at.y;
      const distance = Math.hypot(dx, dy);
      if (distance > 140 || distance === 0) return;
      const impulse = (1 - distance / 140) * 950;
      wakeBody(index);
      body.vx += (dx / distance) * impulse;
      body.vy += (dy / distance) * impulse;
      body.vr += (Math.random() - 0.5) * impulse * 0.5;
    });
  };

  const hop = (index: number) => {
    const body = bodies.current[index];
    if (!body) return;
    wakeBody(index);
    setStyle(chipRefs.current[index], { zIndex: "2" });
    body.mode = "flying";
    body.grounded = false;
    body.knocks = 0;
    body.vx = jitter(180);
    body.vy = -720;
    body.vr = Math.sign(jitter(1)) * 360;
    body.vq -= 6;
    tick();
  };

  const popChip = (index: number) => {
    const body = bodies.current[index];
    if (!body) return;
    if (!poppable(index)) {
      hop(index);
      return;
    }
    const at = { x: body.x, y: body.y };
    burst(at, STATE_COLOR[PILE[index].state]);
    shockwave(at, index);
    hitStop(50);
    sendAway(index, { pop: true });
    tick();
  };

  const heapTopAt = (g: Geometry, body: Body, self: number) => {
    let top = g.h - body.halfH;
    bodies.current.forEach((other, index) => {
      if (
        index !== self &&
        other.mode === "rest" &&
        Math.abs(other.home.x - body.x) < other.half * 0.8
      )
        top = Math.min(top, other.home.y - 18);
    });
    return top;
  };

  const kickNeighbours = (self: number, body: Body, now: number) => {
    const speed = Math.hypot(body.vx, body.vy);
    if (speed < KICK_SPEED) return;
    bodies.current.forEach((other, index) => {
      if (index === self || other.mode !== "rest" || now - other.kickedAt < 260) return;
      const dx = other.x - body.x;
      const dy = other.y - body.y;
      if (Math.abs(dx) > (body.half + other.half) * 0.7 || Math.abs(dy) > 24) return;
      const distance = Math.hypot(dx, dy) || 1;
      const impulse = Math.min(speed * 0.3, 900);
      wakeBody(index);
      other.kickedAt = now;
      other.vx += (dx / distance) * impulse + body.vx * 0.12;
      other.vy += (dy / distance) * impulse + body.vy * 0.12;
      other.vr += jitter(impulse * 0.6);
      if (impulse > KNOCK_LOOSE && body.knocks < MAX_KNOCKS) {
        body.knocks += 1;
        other.mode = "flying";
        other.grounded = false;
        other.knocks = MAX_KNOCKS;
        other.vy -= 220;
        setStyle(chipRefs.current[index], { zIndex: "2" });
        body.vx *= 0.85;
        body.vy *= 0.85;
        return;
      }
      body.vx *= 0.97;
      body.vy *= 0.97;
    });
  };

  const stepHeld = (g: Geometry, body: Body, state: Hold, h: number) => {
    const reach = armOf(body, state.local);
    const target = {
      x: clamp(state.target.x, body.half - reach.x, g.w - body.half - reach.x),
      y: clamp(state.target.y, body.halfH - reach.y, g.h - body.halfH - reach.y),
    };
    const w = 2 * Math.PI * PIVOT_HZ;
    const ax = w * w * (target.x - state.pivot.x) - 2 * w * state.pivotV.x;
    const ay = w * w * (target.y - state.pivot.y) - 2 * w * state.pivotV.y;
    state.pivotV.x += ax * h;
    state.pivotV.y += ay * h;
    state.pivot.x += state.pivotV.x * h;
    state.pivot.y += state.pivotV.y * h;
    [body.s, body.vs] = spring(body.s, body.vs, 1.12, 8, 0.55, h);
    [body.q, body.vq] = spring(body.q, body.vq, 0, 9, 0.35, h);
    const arm = armOf(body, state.local);
    const inertia = body.k2 * body.s * body.s + arm.x * arm.x + arm.y * arm.y;
    const forceX = -ax * INERTIAL_COUPLING - HELD_AIR * body.vx;
    const forceY = HELD_GRAVITY - ay * INERTIAL_COUPLING - HELD_AIR * body.vy;
    const torque = arm.x * forceY - arm.y * forceX;
    const damping = 2 * SWING_ZETA * Math.sqrt(UPRIGHT * inertia);
    let omega = body.vr * RAD;
    omega += ((torque - UPRIGHT * (body.r - state.upright) * RAD - damping * omega) / inertia) * h;
    omega = clamp(omega, -MAX_SWING, MAX_SWING);
    body.r += (omega * h) / RAD;
    body.vr = omega / RAD;
    const next = armOf(body, state.local);
    body.x = state.pivot.x + next.x;
    body.y = state.pivot.y + next.y;
    body.vx = state.pivotV.x - omega * next.y;
    body.vy = state.pivotV.y + omega * next.x;
  };

  const land = (g: Geometry, index: number, body: Body, previous: Point, h: number) => {
    const heapTop = heapTopAt(g, body, index);
    const ground = g.h - body.halfH;
    if (body.grounded) {
      const support = body.y >= ground - 1 ? ground : heapTop;
      if (support > body.y + 2) {
        body.grounded = false;
        return;
      }
      if (support < body.y - 8) {
        body.x = previous.x;
        body.vx = -body.vx * 0.3;
      } else body.y = support;
      body.vy = 0;
      const speed = Math.abs(body.vx);
      body.vx = Math.sign(body.vx) * Math.max(0, speed - FRICTION * h);
      [body.r, body.vr] = spring(body.r, body.vr, body.home.r, 4, 0.75, h);
      if (Math.abs(body.vx) < 4 && Math.abs(body.r - body.home.r) < 0.5 && Math.abs(body.vr) < 6) {
        body.mode = "rest";
        body.grounded = false;
        body.home = { x: body.x, y: body.y, r: body.home.r };
        body.push = { x: 0, y: 0 };
        body.vx = 0;
      }
      return;
    }
    const floor = previous.y <= heapTop + 1 ? heapTop : ground;
    if (body.y <= floor || body.vy <= 0) return;
    body.y = floor;
    body.r = wrapAngle(body.r);
    const impact = body.vy;
    const flat = Math.round(body.r / 180) * 180;
    body.vr -= Math.sin(2 * (body.r - flat) * RAD) * impact * TUMBLE;
    body.vq += Math.min(impact * 0.0095, 14);
    if (impact > BOUNCE_MIN) {
      body.vy = -impact * RESTITUTION;
      body.vx *= 0.8;
      body.vr *= 0.7;
      return;
    }
    body.grounded = true;
    body.vy = 0;
    body.vr *= 0.4;
    body.home.r = flat + jitter(12);
  };

  // Returns false when the chip left the simulation (caught by the mascot).
  const stepFlight = (g: Geometry, index: number, body: Body, h: number, now: number) => {
    const catchRadius = g.mascotSize * 0.45;
    const toMascot = { x: g.mascot.x - body.x, y: g.mascot.y - body.y };
    const distance = Math.hypot(toMascot.x, toMascot.y);
    if (distance < catchRadius) {
      catchChip(index);
      return false;
    }
    const reach = catchRadius * 2.4;
    if (!body.grounded && distance < reach && body.vx * toMascot.x + body.vy * toMascot.y > 0) {
      const pull = (MAGNET * (1 - distance / reach)) / distance;
      body.vx += toMascot.x * pull * h;
      body.vy += toMascot.y * pull * h;
    }
    if (!body.grounded) {
      body.vy += GRAVITY * h;
      const drag = Math.exp(-AIR_DRAG * h);
      body.vx *= drag;
      body.vy *= drag;
      body.vr *= Math.exp(-SPIN_DRAG * h);
    }
    const previous = { x: body.x, y: body.y };
    body.x += body.vx * h;
    body.y += body.vy * h;
    body.r += body.vr * h;
    [body.s, body.vs] = spring(body.s, body.vs, 1, 5, 0.4, h);
    [body.q, body.vq] = spring(body.q, body.vq, 0, 9, 0.35, h);
    if (body.x < body.half || body.x > g.w - body.half) {
      body.x = clamp(body.x, body.half, g.w - body.half);
      body.vq -= Math.min(Math.abs(body.vx) * 0.006, 10);
      body.vx = -body.vx * 0.55;
      body.vr = -body.vr * 0.6;
    }
    kickNeighbours(index, body, now);
    land(g, index, body, previous, h);
    return true;
  };

  const stepRest = (body: Body, dt: number) => {
    [body.x, body.vx] = spring(body.x, body.vx, body.home.x + body.push.x, 6, 0.5, dt);
    [body.y, body.vy] = spring(body.y, body.vy, body.home.y + body.push.y, 6, 0.5, dt);
    [body.r, body.vr] = spring(body.r, body.vr, body.home.r, 5, 0.45, dt);
    [body.s, body.vs] = spring(body.s, body.vs, 1, 5, 0.45, dt);
    [body.q, body.vq] = spring(body.q, body.vq, 0, 9, 0.35, dt);
    const settled =
      Math.abs(body.x - body.home.x - body.push.x) < 0.1 &&
      Math.abs(body.y - body.home.y - body.push.y) < 0.1 &&
      Math.abs(body.r - body.home.r) < 0.1 &&
      Math.abs(body.s - 1) < 0.002 &&
      Math.abs(body.q) < 0.002 &&
      Math.hypot(body.vx, body.vy) < 2 &&
      Math.abs(body.vr) < 2 &&
      Math.abs(body.vs) < 0.01 &&
      Math.abs(body.vq) < 0.01;
    if (settled) body.awake = false;
  };

  const stepBody = (g: Geometry, index: number, dt: number, now: number) => {
    const body = bodies.current[index];
    if (body.mode === "rest") stepRest(body, dt);
    else {
      const steps = Math.max(1, Math.ceil(dt / SUBSTEP));
      const h = dt / steps;
      for (let i = 0; i < steps; i++) {
        const state = hold.current;
        if (body.mode === "held" && state) {
          stepHeld(g, body, state, h);
          kickNeighbours(index, body, now);
        } else if (body.mode !== "flying" || !stepFlight(g, index, body, h, now)) break;
      }
      if (body.mode === "held")
        mascot.current.open =
          Math.hypot(body.x - g.mascot.x, body.y - g.mascot.y) < g.mascotSize * 0.81;
    }
    if (body.mode === "gone") return;
    const pose = { x: body.x, y: body.y, r: body.r, s: body.s };
    setStyle(chipRefs.current[index], {
      transform:
        Math.abs(body.q) > 0.001 ? squash(pose, 1 + body.q, 1 - body.q) : poseTransform(pose),
    });
  };

  const stepMascot = (g: Geometry, dt: number) => {
    const state = mascot.current;
    if (!state.awake) return;
    [state.s, state.vs] = spring(state.s, state.vs, state.open ? 1.08 : 1, 5, 0.42, dt);
    [state.r, state.vr] = spring(state.r, state.vr, state.lean, 4, 0.6, dt);
    setStyle(mascotRef.current, {
      transform: poseTransform({ ...g.mascot, r: state.r, s: state.s }),
    });
    const target = state.open ? 1.08 : 1;
    if (
      Math.abs(state.s - target) < 0.002 &&
      Math.abs(state.r - state.lean) < 0.05 &&
      Math.abs(state.vs) < 0.01 &&
      Math.abs(state.vr) < 0.5
    )
      state.awake = false;
  };

  function loop(now: number) {
    const g = geo.current;
    frameId.current = 0;
    if (!g) return;
    const realDt = Math.min((now - lastTime.current) / 1000, 1 / 30);
    lastTime.current = now;
    const dt = now < slowUntil.current ? realDt * 0.06 : realDt;
    let active = hold.current !== null;
    bodies.current.forEach((body, index) => {
      if (body.mode === "gone" || !body.awake) return;
      stepBody(g, index, dt, now);
      if (body.awake) active = true;
    });
    stepMascot(g, dt);
    if ((active || mascot.current.awake) && !frameId.current)
      frameId.current = requestAnimationFrame(loop);
  }

  const tick = () => {
    if (frameId.current) return;
    lastTime.current = clock();
    frameId.current = requestAnimationFrame(loop);
  };

  const resetBodies = (g: Geometry) => {
    const heap = heapPoses(g);
    bodies.current = heap.map((pose, index) => {
      const width = chipRefs.current[index]?.offsetWidth ?? 120;
      const height = chipRefs.current[index]?.offsetHeight ?? 28;
      return {
        mode: collected.current.has(index) ? "gone" : "rest",
        awake: false,
        grounded: false,
        x: pose.x,
        y: pose.y,
        vx: 0,
        vy: 0,
        r: pose.r ?? 0,
        vr: 0,
        s: 1,
        vs: 0,
        q: 0,
        vq: 0,
        home: { x: pose.x, y: pose.y, r: pose.r ?? 0 },
        push: { x: 0, y: 0 },
        half: width / 2,
        halfH: height / 2,
        k2: (width * width + height * height) / 12,
        kickedAt: 0,
        knocks: 0,
      };
    });
  };

  const showMessy = (g: Geometry) => {
    const heap = heapPoses(g);
    PILE.forEach((chip, index) => {
      const element = chipRefs.current[index];
      setStyle(element, { zIndex: "" });
      place(element, collected.current.has(index) ? targetPose(g, chip.id) : heap[index]);
    });
    place(mascotRef.current, g.mascot);
    setCount(countRef, parked.current);
  };

  const playIntro = (g: Geometry) => {
    const heap = heapPoses(g);
    PILE.forEach((_, index) => {
      const to = heap[index];
      const element = chipRefs.current[index];
      place(element, { ...to, y: to.y - 18, o: 0 });
      fly(element, [frame(to)], { duration: 420, delay: rand(index, 11) * 520 });
    });
    place(mascotRef.current, { ...g.mascot, y: g.mascot.y + 20, s: 0.94, o: 0 });
    fly(mascotRef.current, [frame(g.mascot)], { duration: 480, delay: 560 });
  };

  const tidyAll = (g: Geometry) => {
    const center = g.mascot;
    bodies.current.forEach((body, index) =>
      sendAway(index, { delay: Math.hypot(body.x - center.x, body.y - center.y) * 0.6 }),
    );
  };

  const messUp = (g: Geometry) => {
    collected.current.clear();
    parked.current = 0;
    setCollectedCount(0);
    setCount(countRef, 0);
    const heap = heapPoses(g);
    resetBodies(g);
    bodies.current.forEach((body) => {
      body.mode = "gone";
    });
    PILE.forEach((_, index) => {
      const element = chipRefs.current[index];
      setStyle(element, { zIndex: "" });
      fly(
        element,
        [frame(heap[index])],
        { duration: 420, delay: (PILE.length - index) * 3 },
        () => {
          const body = bodies.current[index];
          if (body && !collected.current.has(index)) body.mode = "rest";
        },
      );
    });
  };

  const onStageSize = useEffectEvent((size: { w: number; h: number }) => {
    const g = geometry(size.w, size.h);
    geo.current = g;
    resetBodies(g);
    if (!started.current) {
      started.current = true;
      setReady(true);
      if (reduced || introSeen(INTRO)) showMessy(g);
      else {
        markIntroSeen(INTRO);
        playIntro(g);
      }
      return;
    }
    showMessy(g);
  });

  useLayoutEffect(() => {
    if (size) onStageSize(size);
  }, [size]);

  const stagePoint = (event: React.PointerEvent): Point | null => {
    const stage = stageRef.current;
    if (!stage) return null;
    const box = stage.getBoundingClientRect();
    return { x: event.clientX - box.left, y: event.clientY - box.top };
  };

  const chipIndexAt = (target: EventTarget) =>
    target instanceof Node
      ? chipRefs.current.findIndex((element) => element?.contains(target) ?? false)
      : -1;

  const repel = (point: Point | null) => {
    if (reduced) return;
    let changed = false;
    bodies.current.forEach((body, index) => {
      if (body.mode !== "rest") return;
      const dx = body.home.x - (point?.x ?? -9999);
      const dy = body.home.y - (point?.y ?? -9999);
      const distance = Math.hypot(dx, dy);
      const push = distance < REPEL_RADIUS && distance > 0 ? (1 - distance / REPEL_RADIUS) * 26 : 0;
      const x = push ? (dx / distance) * push : 0;
      const y = push ? (dy / distance) * push : 0;
      if (Math.abs(x - body.push.x) + Math.abs(y - body.push.y) < 0.5) return;
      body.push = { x, y };
      wakeBody(index);
      changed = true;
    });
    const g = geo.current;
    if (g) {
      const lean = point ? clamp(((point.x - g.mascot.x) / g.w) * 18, -8, 8) : 0;
      if (Math.abs(lean - mascot.current.lean) > 0.2) {
        mascot.current.lean = lean;
        wakeMascot();
        changed = true;
      }
    }
    if (changed) tick();
  };

  const onPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    const index = chipIndexAt(event.target);
    const body = bodies.current[index];
    const point = stagePoint(event);
    if (index < 0 || !body || body.mode === "gone" || !point || hold.current) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    const cos = Math.cos(body.r * RAD);
    const sin = Math.sin(body.r * RAD);
    const dx = body.x - point.x;
    const dy = body.y - point.y;
    hold.current = {
      index,
      pointerId: event.pointerId,
      start: point,
      local: { x: (cos * dx + sin * dy) / body.s, y: (-sin * dx + cos * dy) / body.s },
      upright: Math.round(body.r / 360) * 360,
      target: point,
      pivot: { ...point },
      pivotV: { x: 0, y: 0 },
      moved: false,
      samples: [{ t: event.timeStamp, ...point }],
    };
    if (body.mode === "flying" && !reduced) grab(index);
  };

  const grab = (index: number) => {
    const body = bodies.current[index];
    wakeBody(index);
    body.mode = "held";
    body.grounded = false;
    body.knocks = 0;
    setStyle(chipRefs.current[index], { zIndex: "2" });
    setStyle(stageRef.current, { cursor: "grabbing" });
    wakeMascot();
    tick();
  };

  const onPointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const state = hold.current;
    const point = stagePoint(event);
    if (!point) return;
    if (!state) {
      if (event.pointerType === "mouse") repel(point);
      return;
    }
    if (event.pointerId !== state.pointerId || reduced) return;
    const box = stageRef.current?.getBoundingClientRect();
    const coalesced = event.nativeEvent.getCoalescedEvents?.() ?? [];
    for (const sample of coalesced.length > 0 ? coalesced : [event.nativeEvent])
      state.samples.push({
        t: sample.timeStamp,
        x: sample.clientX - (box?.left ?? 0),
        y: sample.clientY - (box?.top ?? 0),
      });
    while (state.samples.length > 2 && event.timeStamp - state.samples[0].t > SAMPLE_WINDOW_MS)
      state.samples.shift();
    state.target = point;
    if (state.moved) return;
    if (Math.hypot(point.x - state.start.x, point.y - state.start.y) < DRAG_THRESHOLD) return;
    state.moved = true;
    if (bodies.current[state.index].mode !== "held") grab(state.index);
  };

  const release = (cancelled: boolean, time: number) => {
    const state = hold.current;
    hold.current = null;
    if (!state) return;
    const g = geo.current;
    const body = bodies.current[state.index];
    setStyle(stageRef.current, { cursor: "" });
    mascot.current.open = false;
    if (!g || !body || body.mode === "gone") return;
    if (!state.moved) {
      if (body.mode === "held") body.mode = "flying";
      if (cancelled) return;
      if (reduced) {
        if (!poppable(state.index)) return;
        body.mode = "gone";
        markCollected(state.index);
        place(chipRefs.current[state.index], parkedPose(g));
        parked.current += 1;
        setCount(countRef, parked.current);
        return;
      }
      popChip(state.index);
      return;
    }
    const pointer = cancelled ? { x: 0, y: 0 } : releaseVelocity(state.samples, time);
    const arm = armOf(body, state.local);
    const omega = body.vr * RAD;
    let vx = pointer.x - omega * arm.y;
    let vy = Math.max(pointer.y + omega * arm.x, -MAX_RISE);
    const speed = Math.hypot(vx, vy);
    if (speed > MAX_SPEED) {
      vx *= MAX_SPEED / speed;
      vy *= MAX_SPEED / speed;
    }
    if (Math.hypot(body.x - g.mascot.x, body.y - g.mascot.y) < g.mascotSize * 0.6) {
      catchChip(state.index);
      return;
    }
    body.mode = "flying";
    body.grounded = false;
    body.knocks = 0;
    body.vx = vx;
    body.vy = vy;
    tick();
  };

  const onMascotClick = () => {
    const g = geo.current;
    if (!g) return;
    if (reduced) {
      if (tidy) {
        collected.current.clear();
        parked.current = 0;
        setCollectedCount(0);
        resetBodies(g);
      } else {
        PILE.forEach((_, index) => collected.current.add(index));
        parked.current = PILE_COUNTS.parked;
        setCollectedCount(PILE.length);
      }
      showMessy(g);
      return;
    }
    wakeMascot();
    mascot.current.vs += 2.4;
    tick();
    if (tidy) messUp(g);
    else tidyAll(g);
  };

  return (
    <div
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={(event) => {
        if (event.pointerId === hold.current?.pointerId) release(false, event.timeStamp);
      }}
      onPointerCancel={(event) => {
        if (event.pointerId === hold.current?.pointerId) release(true, event.timeStamp);
      }}
      onPointerLeave={() => {
        if (!hold.current) repel(null);
      }}
    >
      <PileFrame stageRef={stageRef} ready={ready} tidy={tidy}>
        {PILE.map((chip, index) => (
          <PileChipView
            key={chip.id}
            chip={chip}
            tidy={tidy}
            narrow={g?.narrow ?? false}
            chipRef={(element) => {
              chipRefs.current[index] = element;
              setStyle(element, {
                cursor: tidy ? "" : "grab",
                pointerEvents: tidy ? "" : "auto",
                touchAction: "none",
              });
            }}
          />
        ))}
        {g && (
          <PileDrawer
            drawerRef={drawerRef}
            countRef={countRef}
            at={g.drawer}
            open={collectedCount > 0}
          />
        )}
        <PileMascot
          mascotRef={mascotRef}
          size={g?.mascotSize ?? 132}
          label={tidy ? "Make a mess again" : "Tidy up the skills"}
          pressed={tidy}
          onClick={onMascotClick}
        />
        <div
          ref={fxRef}
          aria-hidden="true"
          style={{ inset: 0, pointerEvents: "none", position: "absolute", zIndex: 4 }}
        />
      </PileFrame>
    </div>
  );
}
