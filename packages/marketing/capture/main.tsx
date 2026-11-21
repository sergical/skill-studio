// ============================================================================
// Skill Studio - marketing capture
// Boots the desktop app against the shared harness fixture/mock, then drives
// it into one of a fixed set of scenes for the marketing site's screenshots
// and Remotion walkthroughs. See `apps/desktop/src/dev/harness/` for the
// fixture and mock IPC layer this now shares with the browser dev harness.
// ============================================================================

import { createRoot } from "react-dom/client";
import type { ListingReport, SkillSessionsReport, SkillSnapshot } from "../../lib/src/index.ts";
import {
  buildHarnessSnapshot,
  HARNESS_PROJECT,
} from "../../../apps/desktop/src/dev/harness/skill-fixture.ts";
import { installMockTauri } from "../../../apps/desktop/src/dev/harness/mock-tauri.ts";

type Scene =
  | "map"
  | "coverage"
  | "invoke"
  | "drift"
  | "install"
  | "activity"
  | "repair"
  | "listing"
  | "sessions";

const SCENES: readonly Scene[] = [
  "map",
  "coverage",
  "invoke",
  "drift",
  "install",
  "activity",
  "repair",
  "listing",
  "sessions",
];

interface CaptureControl {
  setScene: (scene: Scene) => void;
  snapshot: () => SkillSnapshot;
}

declare global {
  interface Window {
    __captureReady?: boolean;
    __capture?: CaptureControl;
  }
}

const control = installMockTauri(buildHarnessSnapshot());

// The dev fixture names its sessions "Session 1". A video shows prompts a person would type.
const CAPTURE_PROMPTS = new Map([
  ["s1", "commit the lint fixes"],
  ["s2", "why is the build slow on CI"],
]);

// The dev fixture's budget leaves no skill with a description. At this one, about half keep it.
const CAPTURE_FRACTION = 0.0005;

function sessionsForCapture(report: SkillSessionsReport): SkillSessionsReport {
  return {
    ...report,
    sessions: report.sessions.map((session) => ({
      ...session,
      prompt: CAPTURE_PROMPTS.get(session.id ?? "") ?? session.prompt,
    })),
  };
}

// Words the Claude Code listing the way the core does, in place of the fixture's "fixture"
// labels.
function listingForCapture(report: ListingReport, fraction: number, tried: boolean): ListingReport {
  const source = tried ? "chosen in Skill Studio" : "~/.claude/settings.json";
  return {
    listings: report.listings.map((row) =>
      row.harness === "claude-code"
        ? {
            ...row,
            budget_source: `${fraction} of 1000000 tokens at 3 characters per token (${source})`,
            context_window_tokens: null,
            setting: row.setting && {
              ...row.setting,
              current: String(CAPTURE_FRACTION),
              chezmoi_source: null,
            },
          }
        : row,
    ),
  };
}

const params = new URLSearchParams(location.search);
const sceneParam = params.get("scene");
const initialScene = SCENES.find((scene) => scene === sceneParam) ?? "map";
const theme = params.get("theme") === "light" ? "light" : "dark";
localStorage.setItem("theme", theme);
localStorage.setItem("project-paths", HARNESS_PROJECT);
document.documentElement.setAttribute("data-theme", theme);

const [{ default: App }, { useAppStore }] = await Promise.all([
  import("../../../apps/desktop/src/App.tsx"),
  import("../../../apps/desktop/src/store/appStore.ts"),
]);

// The Sessions and Model listing pages read through these two store actions.
const { readSkillSessions, readListing } = useAppStore.getState();
useAppStore.setState({
  readSkillSessions: async (request) => sessionsForCapture(await readSkillSessions(request)),
  readListing: async (request) => {
    const fraction = request.claude_budget_fraction ?? CAPTURE_FRACTION;
    const report = await readListing({ ...request, claude_budget_fraction: fraction });
    return listingForCapture(report, fraction, request.claude_budget_fraction != null);
  },
});

function setScene(scene: Scene): void {
  const common = {
    userAddedProjects: [HARNESS_PROJECT],
    excludedProjects: [],
    addSkillSheet: { open: false },
  };
  if (scene === "coverage") {
    useAppStore.setState({
      ...common,
      activeView: { kind: "skills" },
      showCoverage: true,
      skillListFilter: { ...useAppStore.getState().skillListFilter, query: "", scope: "all" },
    });
  } else if (scene === "activity") {
    useAppStore.setState({ ...common, activeView: { kind: "activity" }, usageWindow: "30d" });
  } else if (scene === "listing" || scene === "sessions") {
    useAppStore.setState({ ...common, activeView: { kind: scene } });
  } else if (scene === "install") {
    useAppStore.setState({
      ...common,
      activeView: { kind: "home" },
      addSkillSheet: { open: true, prefill: "anthropics/skills/tree/main/frontend-design" },
    });
  } else if (scene === "repair") {
    useAppStore.setState({
      ...common,
      activeView: {
        kind: "skill",
        name: "release-notes",
        deploymentPath: `${HARNESS_PROJECT}/.claude/skills/release-notes`,
        from: { kind: "home" },
      },
    });
  } else {
    useAppStore.setState({
      ...common,
      activeView: {
        kind: "skill",
        name: "commit",
        from: { kind: "skills" },
        intent: scene === "drift" ? "compare" : undefined,
      },
    });
  }
}

setScene(initialScene);
window.__capture = { setScene, snapshot: () => control.snapshot() };
createRoot(document.getElementById("root")!).render(<App />);
requestAnimationFrame(() =>
  requestAnimationFrame(() => {
    window.__captureReady = true;
  }),
);
