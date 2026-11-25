// ============================================================================
// Skill Studio - Interactive CLI & MCP panels for the hero surface showcase
// Redesigned terminal & coding-agent views with structured output, visual
// budget bars, interactive command/prompt switching, and syntax-colored JSON.
// ============================================================================
import { useState } from "react";
import * as stylex from "@stylexjs/stylex";
import {
  Check,
  ChevronDown,
  ChevronRight,
  CornerDownRight,
  RotateCcw,
  Sparkles,
  Wrench,
} from "lucide-react";

import { AgentIcon } from "../../AgentIcon";
import { siteTokens } from "../../SiteTheme.stylex";

export type CliCommandId = "listing" | "sessions" | "diagnose" | "usage" | "park";

export interface CliCommandSpec {
  id: CliCommandId;
  label: string;
  cmd: string;
  timing: string;
  summary: string;
}

export const CLI_COMMANDS: ReadonlyArray<CliCommandSpec> = [
  {
    id: "listing",
    label: "listing",
    cmd: "npx skill-studio@latest listing",
    timing: "42ms",
    summary: "5 described · 12 name only · 14 hidden (16,000 char budget)",
  },
  {
    id: "sessions",
    label: "sessions",
    cmd: "npx skill-studio@latest sessions --days 30",
    timing: "118ms",
    summary: "142 sessions scanned · 12 never triggered · 1 possible miss",
  },
  {
    id: "diagnose",
    label: "diagnose",
    cmd: "npx skill-studio@latest diagnose",
    timing: "31ms",
    summary: "2 errors · 1 warning — all auto-fixable",
  },
  {
    id: "usage",
    label: "usage",
    cmd: "npx skill-studio@latest usage --days 30",
    timing: "64ms",
    summary: "19 active · 12 idle in last 30 days (14,820 chars reclaimable)",
  },
  {
    id: "park",
    label: "park",
    cmd: "npx skill-studio@latest park --unused-days 30",
    timing: "27ms",
    summary: "12 parked · 16 of 19 now described · undoable any time",
  },
];

const LISTING_ROWS = [
  { state: "described" as const, name: "commit", chars: "1,420", bar: 86 },
  { state: "described" as const, name: "react-perf-audit", chars: "1,180", bar: 72 },
  { state: "described" as const, name: "agent-browser", chars: "980", bar: 60 },
  { state: "nameOnly" as const, name: "writing-skills", chars: "118", bar: 12 },
  {
    state: "nameOnly" as const,
    name: "opentelemetry-browser-instrumentation-guide",
    chars: "94",
    bar: 9,
  },
  { state: "hidden" as const, name: "deploy-preview", chars: "0", bar: 0 },
  { state: "hidden" as const, name: "release-notes", chars: "0", bar: 0 },
];

const SESSION_ROWS = [
  {
    name: "writing-skills",
    listedAs: "name only",
    sessions: "142",
    outcome: "missed 3 (guess)",
    tone: "warn" as const,
  },
  {
    name: "commit",
    listedAs: "described",
    sessions: "142",
    outcome: "used 48",
    tone: "ok" as const,
  },
  {
    name: "agent-browser",
    listedAs: "described",
    sessions: "119",
    outcome: "used 31",
    tone: "ok" as const,
  },
  {
    name: "release-notes",
    listedAs: "name only",
    sessions: "142",
    outcome: "used 0",
    tone: "muted" as const,
  },
  {
    name: "opentelemetry-browser-instrumentation-guide",
    listedAs: "name only",
    sessions: "142",
    outcome: "used 0",
    tone: "muted" as const,
  },
  {
    name: "deploy-preview",
    listedAs: "hidden",
    sessions: "0",
    outcome: "used 0",
    tone: "muted" as const,
  },
];

const DIAGNOSE_ITEMS = [
  {
    level: "error" as const,
    code: "broken-symlink",
    title: "pdf-tools points to a folder that no longer exists",
    path: "~/.claude/skills/pdf-tools -> ~/old-repo/.agents/skills/pdf-tools",
    help: "relink to healthy global copy at ~/.agents/skills/pdf-tools",
  },
  {
    level: "error" as const,
    code: "invalid-frontmatter",
    title: "motion/SKILL.md is missing its closing `---` fence",
    path: "~/projects/agent-studio/.agents/skills/motion/SKILL.md:4",
    help: "close YAML frontmatter so Claude Code and Codex can index it",
  },
  {
    level: "warn" as const,
    code: "shadowed-copy",
    title: "deploy-preview exists in both global and project scope",
    path: "~/projects/agent-studio/.agents/skills/deploy-preview",
    help: "compare copies or disable one agent scope",
  },
];

const USAGE_ROWS = [
  { name: "commit", runs: 48, share: 100, last: "14m ago", active: true },
  { name: "agent-browser", runs: 31, share: 65, last: "2h ago", active: true },
  { name: "tailwind-v4", runs: 19, share: 40, last: "yesterday", active: true },
  { name: "writing-skills", runs: 2, share: 8, last: "18d ago", active: true },
  { name: "release-notes", runs: 0, share: 0, last: "never used", active: false },
  { name: "deploy-preview", runs: 0, share: 0, last: "never used", active: false },
];

const CLI_BY_ID = new Map(CLI_COMMANDS.map((cmd) => [cmd.id, cmd]));

export function HeroCliWindow({
  activeId,
  onSelectCommand,
}: {
  activeId: CliCommandId;
  onSelectCommand: (next: CliCommandId) => void;
}) {
  const [replayCount, setReplayCount] = useState(0);
  const active = CLI_BY_ID.get(activeId) ?? CLI_COMMANDS[0];

  const selectCmd = (next: CliCommandId) => {
    onSelectCommand(next);
    setReplayCount((c) => c + 1);
  };

  return (
    <div {...stylex.props(styles.window, styles.cliWindow)}>
      <div {...stylex.props(styles.titlebar)}>
        <span {...stylex.props(styles.lights)} aria-hidden="true">
          <span {...stylex.props(styles.light)} />
          <span {...stylex.props(styles.light)} />
          <span {...stylex.props(styles.light)} />
        </span>
        <span {...stylex.props(styles.titlebarText)}>~/projects/agent-studio — zsh</span>
        <button
          type="button"
          onClick={() => setReplayCount((c) => c + 1)}
          {...stylex.props(styles.replayBtn)}
        >
          <RotateCcw size={13} aria-hidden="true" />
          <span>Replay</span>
        </button>
      </div>

      <div {...stylex.props(styles.subHeader)}>
        <div role="group" aria-label="CLI commands" {...stylex.props(styles.cmdStrip)}>
          {CLI_COMMANDS.map((cmd) => {
            const on = cmd.id === activeId;
            return (
              <button
                key={cmd.id}
                type="button"
                aria-pressed={on}
                onClick={() => selectCmd(cmd.id)}
                {...stylex.props(styles.cmdTab, on && styles.cmdTabOn)}
              >
                <span {...stylex.props(styles.cmdTabPrefix, on && styles.cmdTabPrefixOn)}>$</span>
                <span>{cmd.label}</span>
              </button>
            );
          })}
        </div>
        <span {...stylex.props(styles.metaBadge)}>
          <span {...stylex.props(styles.greenDot)} aria-hidden="true" />
          <span>exit 0 · {active.timing}</span>
        </span>
      </div>

      <div key={`${active.id}:${replayCount}`} {...stylex.props(styles.termBody)}>
        {/* Command prompt header */}
        <div {...stylex.props(styles.promptRow, styles.lineReveal)}>
          <span {...stylex.props(styles.promptPath)}>~/projects/agent-studio</span>
          <span {...stylex.props(styles.promptBranch)}>main</span>
          <span {...stylex.props(styles.promptCaret)}>❯</span>
          <span {...stylex.props(styles.promptCmd)}>{active.cmd}</span>
        </div>

        {active.id === "listing" ? (
          <div {...stylex.props(styles.termSection)}>
            <div {...stylex.props(styles.budgetCard, styles.lineReveal, dynamic.delay("40ms"))}>
              <div {...stylex.props(styles.budgetHeader)}>
                <span {...stylex.props(styles.budgetTitle)}>
                  claude-code · global · 16,000 char listing budget
                </span>
                <span {...stylex.props(styles.budgetRight)}>31 skills installed</span>
              </div>
              <div {...stylex.props(styles.budgetBar)} aria-hidden="true">
                <span {...stylex.props(styles.segDescribed)} />
                <span {...stylex.props(styles.segNameOnly)} />
                <span {...stylex.props(styles.segHidden)} />
              </div>
              <div {...stylex.props(styles.budgetLegend)}>
                <span {...stylex.props(styles.legendItem)}>
                  <span {...stylex.props(styles.legendSwatch, styles.swatchOk)} />5 described
                </span>
                <span {...stylex.props(styles.legendItem)}>
                  <span {...stylex.props(styles.legendSwatch, styles.swatchWarn)} />
                  12 name only
                </span>
                <span {...stylex.props(styles.legendItem)}>
                  <span {...stylex.props(styles.legendSwatch, styles.swatchErr)} />
                  14 hidden
                </span>
              </div>
            </div>

            <div {...stylex.props(styles.cliTable)}>
              <div {...stylex.props(styles.cliTableHead)}>
                <span>STATE</span>
                <span>SKILL</span>
                <span {...stylex.props(styles.alignRight)}>PROMPT WEIGHT</span>
              </div>
              {LISTING_ROWS.map((row, idx) => (
                <div
                  key={row.name}
                  {...stylex.props(
                    styles.cliTableRow,
                    styles.lineReveal,
                    dynamic.delay(`${70 + idx * 24}ms`),
                  )}
                >
                  <span
                    {...stylex.props(
                      styles.stateTag,
                      row.state === "described" && styles.stateTagOk,
                      row.state === "nameOnly" && styles.stateTagWarn,
                      row.state === "hidden" && styles.stateTagErr,
                    )}
                  >
                    {row.state === "nameOnly" ? "name only" : row.state}
                  </span>
                  <span {...stylex.props(styles.skillNameCell)}>{row.name}</span>
                  <span {...stylex.props(styles.weightCell)}>
                    <span {...stylex.props(styles.miniBarTrack)} aria-hidden="true">
                      <span
                        {...stylex.props(
                          styles.miniBarFill,
                          row.state === "described" && styles.swatchOk,
                          row.state === "nameOnly" && styles.swatchWarn,
                          row.state === "hidden" && styles.swatchErr,
                          dynamic.width(`${row.bar}%`),
                        )}
                      />
                    </span>
                    <span {...stylex.props(styles.charsNum)}>{row.chars} chars</span>
                  </span>
                </div>
              ))}
            </div>

            <button
              type="button"
              onClick={() => selectCmd("park")}
              {...stylex.props(styles.nextTipBtn, styles.lineReveal, dynamic.delay("230ms"))}
            >
              <CornerDownRight size={14} aria-hidden="true" />
              <span>
                12 unused skills hold 14,820 chars. Try{" "}
                <strong {...stylex.props(styles.tipStrong)}>
                  npx skill-studio@latest park --unused-days 30
                </strong>
              </span>
            </button>
          </div>
        ) : active.id === "sessions" ? (
          <div {...stylex.props(styles.termSection)}>
            <div {...stylex.props(styles.statRibbon, styles.lineReveal, dynamic.delay("40ms"))}>
              <span {...stylex.props(styles.statPill)}>142 sessions · 6 projects</span>
              <span {...stylex.props(styles.statPillWarn)}>1 possible miss</span>
              <span {...stylex.props(styles.statPillMuted)}>12 listed, never used</span>
            </div>
            <div {...stylex.props(styles.cliTable)}>
              <div {...stylex.props(styles.cliSessionHead)}>
                <span>SKILL</span>
                <span {...stylex.props(styles.hideMobile)}>LISTED AS</span>
                <span {...stylex.props(styles.alignRight)}>SESSIONS</span>
                <span {...stylex.props(styles.alignRight)}>OUTCOME</span>
              </div>
              {SESSION_ROWS.map((row, idx) => (
                <div
                  key={row.name}
                  {...stylex.props(
                    styles.cliSessionRow,
                    styles.lineReveal,
                    dynamic.delay(`${70 + idx * 24}ms`),
                  )}
                >
                  <span {...stylex.props(styles.skillNameCell)}>{row.name}</span>
                  <span {...stylex.props(styles.mutedMono, styles.hideMobile)}>{row.listedAs}</span>
                  <span {...stylex.props(styles.alignRight, styles.mutedMono)}>{row.sessions}</span>
                  <span
                    {...stylex.props(
                      styles.alignRight,
                      row.tone === "ok" && styles.toneOk,
                      row.tone === "warn" && styles.toneWarn,
                      row.tone === "muted" && styles.mutedMono,
                    )}
                  >
                    {row.outcome}
                  </span>
                </div>
              ))}
            </div>
            <button
              type="button"
              onClick={() => selectCmd("listing")}
              {...stylex.props(styles.nextTipBtn, styles.lineReveal, dynamic.delay("210ms"))}
            >
              <CornerDownRight size={14} aria-hidden="true" />
              <span>
                writing-skills lost its description in 142 sessions. Inspect budget with{" "}
                <strong {...stylex.props(styles.tipStrong)}>listing</strong>
              </span>
            </button>
          </div>
        ) : active.id === "diagnose" ? (
          <div {...stylex.props(styles.termSection)}>
            <div {...stylex.props(styles.diagList)}>
              {DIAGNOSE_ITEMS.map((item, idx) => (
                <div
                  key={item.code}
                  {...stylex.props(
                    styles.diagCard,
                    item.level === "error" ? styles.diagCardErr : styles.diagCardWarn,
                    styles.lineReveal,
                    dynamic.delay(`${50 + idx * 45}ms`),
                  )}
                >
                  <div {...stylex.props(styles.diagTop)}>
                    <span
                      {...stylex.props(
                        styles.diagLevel,
                        item.level === "error" ? styles.toneErr : styles.toneWarn,
                      )}
                    >
                      {item.level}[{item.code}]
                    </span>
                    <span {...stylex.props(styles.diagTitle)}>{item.title}</span>
                  </div>
                  <div {...stylex.props(styles.diagPath)}>--&gt; {item.path}</div>
                  <div {...stylex.props(styles.diagHelp)}>help: {item.help}</div>
                </div>
              ))}
            </div>
            <div {...stylex.props(styles.diagFoot, styles.lineReveal, dynamic.delay("190ms"))}>
              <span>2 errors, 1 warning found across 31 skills.</span>
              <span {...stylex.props(styles.toneAccent)}>
                Run `npx skill-studio@latest fix --dry-run` to preview repairs.
              </span>
            </div>
          </div>
        ) : active.id === "usage" ? (
          <div {...stylex.props(styles.termSection)}>
            <div {...stylex.props(styles.statRibbon, styles.lineReveal, dynamic.delay("40ms"))}>
              <span {...stylex.props(styles.statPillOk)}>19 active skills</span>
              <span {...stylex.props(styles.statPillWarn)}>12 unused in 30 days</span>
              <span {...stylex.props(styles.statPillMuted)}>14,820 chars idle</span>
            </div>
            <div {...stylex.props(styles.usageList)}>
              {USAGE_ROWS.map((row, idx) => (
                <div
                  key={row.name}
                  {...stylex.props(
                    styles.usageRow,
                    styles.lineReveal,
                    dynamic.delay(`${70 + idx * 24}ms`),
                  )}
                >
                  <span {...stylex.props(styles.skillNameCell)}>{row.name}</span>
                  <span {...stylex.props(styles.usageBarWrap)} aria-hidden="true">
                    <span
                      {...stylex.props(
                        styles.usageBarFill,
                        row.active ? styles.swatchOk : styles.swatchMuted,
                        dynamic.width(`${row.share}%`),
                      )}
                    />
                  </span>
                  <span {...stylex.props(styles.usageCount)}>
                    {row.runs} {row.runs === 1 ? "run" : "runs"}
                  </span>
                  <span {...stylex.props(styles.usageLast)}>{row.last}</span>
                </div>
              ))}
            </div>
            <button
              type="button"
              onClick={() => selectCmd("park")}
              {...stylex.props(styles.nextTipBtn, styles.lineReveal, dynamic.delay("210ms"))}
            >
              <CornerDownRight size={14} aria-hidden="true" />
              <span>
                Park the 12 idle skills with{" "}
                <strong {...stylex.props(styles.tipStrong)}>
                  npx skill-studio@latest park --unused-days 30
                </strong>
              </span>
            </button>
          </div>
        ) : (
          <div {...stylex.props(styles.termSection)}>
            <div
              {...stylex.props(styles.parkSuccessCard, styles.lineReveal, dynamic.delay("45ms"))}
            >
              <div {...stylex.props(styles.parkBanner)}>
                <Check size={16} aria-hidden="true" />
                <span>Parked 12 unused skills to ~/.skill-studio/parked/</span>
              </div>
              <div {...stylex.props(styles.parkList)}>
                release-notes, deploy-preview, pdf-tools,
                opentelemetry-browser-instrumentation-guide, prisma-migrate + 7 more
              </div>
            </div>

            <div {...stylex.props(styles.deltaGrid, styles.lineReveal, dynamic.delay("95ms"))}>
              <div {...stylex.props(styles.deltaCard)}>
                <span {...stylex.props(styles.deltaLabel)}>Described</span>
                <span {...stylex.props(styles.deltaValue, styles.toneOk)}>5 → 16</span>
                <span {...stylex.props(styles.deltaNote)}>+11 full descriptions restored</span>
              </div>
              <div {...stylex.props(styles.deltaCard)}>
                <span {...stylex.props(styles.deltaLabel)}>Name only</span>
                <span {...stylex.props(styles.deltaValue)}>12 → 3</span>
                <span {...stylex.props(styles.deltaNote)}>writing-skills now fits</span>
              </div>
              <div {...stylex.props(styles.deltaCard)}>
                <span {...stylex.props(styles.deltaLabel)}>Hidden</span>
                <span {...stylex.props(styles.deltaValue, styles.toneOk)}>14 → 0</span>
                <span {...stylex.props(styles.deltaNote)}>zero silently dropped</span>
              </div>
            </div>

            <div {...stylex.props(styles.diagFoot, styles.lineReveal, dynamic.delay("150ms"))}>
              <span>Reversible — every file stays intact locally.</span>
              <span {...stylex.props(styles.toneAccent)}>
                Undo any time: `npx skill-studio@latest undo`
              </span>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

// ============================================================================
// Redesigned MCP Server Window (modern coding-agent timeline + colored JSON)
// ============================================================================
type McpScenarioId = "park" | "missed" | "repair";

interface JsonEntry {
  key: string;
  value: string;
  kind: "string" | "number" | "bool";
}

interface McpToolCall {
  name: string;
  args: string;
  timing: string;
  resultSummary: string;
  entries: ReadonlyArray<JsonEntry>;
}

const MCP_TOOLS_LIST = [
  "skill_listing",
  "skill_sessions",
  "skill_usage",
  "diagnose",
  "park_skills",
  "undo",
] as const;

const MCP_SCENARIOS: ReadonlyArray<{
  id: McpScenarioId;
  label: string;
  prompt: string;
  usedTools: ReadonlyArray<string>;
  tools: ReadonlyArray<McpToolCall>;
  metrics: ReadonlyArray<{ label: string; tone: "ok" | "accent" | "muted" }>;
  metricsAfter: ReadonlyArray<{ label: string; tone: "ok" | "accent" | "muted" }>;
  reply: string;
  followUp: string;
  actionLabel: string;
  actionDoneLabel: string;
  actionDoneReply: string;
}> = [
  {
    id: "park",
    label: "Park unused skills",
    prompt: "Park every skill I haven’t used in 30 days and tell me what fits back in budget.",
    usedTools: ["skill_usage", "park_skills", "undo"],
    tools: [
      {
        name: "skill_usage",
        args: "days: 30",
        timing: "38ms",
        resultSummary: "12 of 31 unused · 14,820 chars",
        entries: [
          { key: "total_skills", value: "31", kind: "number" },
          { key: "unused_count", value: "12", kind: "number" },
          { key: "reclaimable_chars", value: "14820", kind: "number" },
          {
            key: "candidates",
            value: '["release-notes", "deploy-preview", "pdf-tools", "+9 more"]',
            kind: "string",
          },
        ],
      },
      {
        name: "park_skills",
        args: "count: 12, reversible: true",
        timing: "24ms",
        resultSummary: "12 parked · 16 of 19 now described",
        entries: [
          { key: "parked", value: "12", kind: "number" },
          { key: "undo_token", value: '"op_01JV8X2K"', kind: "string" },
          { key: "described_after", value: "16", kind: "number" },
          { key: "hidden_after", value: "0", kind: "number" },
        ],
      },
    ],
    metrics: [
      { label: "12 parked", tone: "accent" },
      { label: "5 → 16 described", tone: "ok" },
      { label: "0 hidden", tone: "ok" },
    ],
    metricsAfter: [
      { label: "12 restored", tone: "ok" },
      { label: "op_01JV8X2K undone", tone: "muted" },
    ],
    reply:
      "Done. I parked 12 idle skills into ~/.skill-studio/parked/. Your agents now load 19 skills with 16 full descriptions and 0 hidden — up from 5 described before.",
    followUp: "motion/SKILL.md also has an unclosed frontmatter header. Want me to repair it?",
    actionLabel: "Undo park",
    actionDoneLabel: "Restored 12 skills",
    actionDoneReply:
      "Restored all 12 skills via undo token op_01JV8X2K. Every original folder and symlink is back exactly where it was.",
  },
  {
    id: "missed",
    label: "Why didn’t a skill fire?",
    prompt: "Why didn’t writing-skills trigger when I asked you to edit SKILL.md?",
    usedTools: ["skill_listing", "skill_sessions"],
    tools: [
      {
        name: "skill_listing",
        args: 'harness: "claude-code", skill: "writing-skills"',
        timing: "29ms",
        resultSummary: "Listed as name-only at 16,000 char cap",
        entries: [
          { key: "skill", value: '"writing-skills"', kind: "string" },
          { key: "visibility", value: '"name_only"', kind: "string" },
          { key: "description_dropped", value: "true", kind: "bool" },
          { key: "idle_skills_ahead", value: "12", kind: "number" },
        ],
      },
    ],
    metrics: [
      { label: "name only", tone: "accent" },
      { label: "missed in 3 sessions", tone: "muted" },
    ],
    metricsAfter: [
      { label: "4 idle parked", tone: "accent" },
      { label: "writing-skills described", tone: "ok" },
    ],
    reply:
      "Claude Code only received the bare name writing-skills — its trigger description was dropped because 12 unused skills ahead of it consumed the 16,000-character listing budget.",
    followUp: "Parking 4 unused skills gives writing-skills its full description back immediately.",
    actionLabel: "Park top 4 blockers",
    actionDoneLabel: "writing-skills restored",
    actionDoneReply:
      "Parked the top 4 idle skills. writing-skills now renders with its full trigger description in every new Claude Code session.",
  },
  {
    id: "repair",
    label: "Fix broken skills",
    prompt: "Check if any of my skills are broken across Claude Code and Codex and fix them.",
    usedTools: ["diagnose"],
    tools: [
      {
        name: "diagnose",
        args: 'scope: "all"',
        timing: "34ms",
        resultSummary: "1 broken symlink · 1 bad frontmatter",
        entries: [
          { key: "checked_skills", value: "31", kind: "number" },
          { key: "broken_symlinks", value: '["pdf-tools"]', kind: "string" },
          { key: "frontmatter_errors", value: '["motion/SKILL.md:4"]', kind: "string" },
          { key: "auto_fixable", value: "true", kind: "bool" },
        ],
      },
    ],
    metrics: [
      { label: "2 issues found", tone: "accent" },
      { label: "both auto-fixable", tone: "ok" },
    ],
    metricsAfter: [
      { label: "2 fixed", tone: "ok" },
      { label: "0 issues remaining", tone: "ok" },
    ],
    reply:
      "Found 2 issues: pdf-tools symlinks to a deleted repo path, and motion/SKILL.md is missing its closing --- frontmatter fence.",
    followUp: "Ready to relink pdf-tools to your global copy and close the frontmatter header.",
    actionLabel: "Apply both fixes",
    actionDoneLabel: "2 issues fixed",
    actionDoneReply:
      "Relinked pdf-tools to ~/.agents/skills/pdf-tools and closed the frontmatter fence in motion/SKILL.md. Re-ran diagnose: 0 issues across 31 skills.",
  },
];

const MCP_BY_ID = new Map(MCP_SCENARIOS.map((item) => [item.id, item]));

export function HeroMcpWindow() {
  const [scenarioId, setScenarioId] = useState<McpScenarioId>("park");
  const [expandedTool, setExpandedTool] = useState<string | null>(null);
  const [actedIds, setActedIds] = useState<Record<string, boolean>>({});
  const scenario = MCP_BY_ID.get(scenarioId) ?? MCP_SCENARIOS[0];
  const acted = Boolean(actedIds[scenario.id]);
  const activeMetrics = acted ? scenario.metricsAfter : scenario.metrics;

  return (
    <div {...stylex.props(styles.window, styles.mcpWindow)}>
      <div {...stylex.props(styles.titlebar)}>
        <div {...stylex.props(styles.mcpTitleLeft)}>
          <span {...stylex.props(styles.lights)} aria-hidden="true">
            <span {...stylex.props(styles.light)} />
            <span {...stylex.props(styles.light)} />
            <span {...stylex.props(styles.light)} />
          </span>
          <span {...stylex.props(styles.agentBadge)}>
            <AgentIcon agent="claude" size={15} />
            <span>Claude Code</span>
            <span {...stylex.props(styles.agentSep)} aria-hidden="true">
              ·
            </span>
            <span {...stylex.props(styles.agentServer)}>skill-studio MCP</span>
          </span>
        </div>
        <span {...stylex.props(styles.metaBadge)}>
          <span {...stylex.props(styles.greenDot)} aria-hidden="true" />
          <span>connected · stdio</span>
        </span>
      </div>

      <div {...stylex.props(styles.subHeader)}>
        <div role="group" aria-label="Agent scenarios" {...stylex.props(styles.cmdStrip)}>
          {MCP_SCENARIOS.map((item) => {
            const on = item.id === scenarioId;
            return (
              <button
                key={item.id}
                type="button"
                aria-pressed={on}
                onClick={() => {
                  setScenarioId(item.id);
                  setExpandedTool(null);
                }}
                {...stylex.props(styles.cmdTab, on && styles.cmdTabOn)}
              >
                <Sparkles
                  size={12}
                  aria-hidden="true"
                  {...stylex.props(on ? styles.cmdTabPrefixOn : styles.cmdTabPrefix)}
                />
                <span>{item.label}</span>
              </button>
            );
          })}
        </div>
        <span {...stylex.props(styles.hintCaption)}>Click a tool call to inspect JSON</span>
      </div>

      <div key={scenario.id} {...stylex.props(styles.mcpBody)}>
        {/* Prompt row */}
        <div {...stylex.props(styles.mcpPromptCard, styles.lineReveal)}>
          <span {...stylex.props(styles.mcpPromptChevron)} aria-hidden="true">
            ›
          </span>
          <span {...stylex.props(styles.mcpPromptText)}>{scenario.prompt}</span>
        </div>

        {/* Vertical tool timeline */}
        <div {...stylex.props(styles.timeline)}>
          {scenario.tools.map((tool, index) => {
            const key = `${scenario.id}:${tool.name}`;
            const open = expandedTool === key;
            return (
              <div
                key={key}
                {...stylex.props(
                  styles.toolNode,
                  styles.lineReveal,
                  dynamic.delay(`${45 + index * 45}ms`),
                )}
              >
                <span {...stylex.props(styles.timelineDot)} aria-hidden="true" />
                <div {...stylex.props(styles.toolCard)}>
                  <button
                    type="button"
                    aria-expanded={open}
                    onClick={() => setExpandedTool(open ? null : key)}
                    {...stylex.props(styles.toolRowBtn)}
                  >
                    <span {...stylex.props(styles.toolHeadLeft)}>
                      {open ? (
                        <ChevronDown size={14} aria-hidden="true" />
                      ) : (
                        <ChevronRight size={14} aria-hidden="true" />
                      )}
                      <span {...stylex.props(styles.toolName)}>skill-studio / {tool.name}</span>
                      <span {...stylex.props(styles.toolArgs)}>{tool.args}</span>
                    </span>
                    <span {...stylex.props(styles.toolHeadRight)}>
                      <span {...stylex.props(styles.toolSummary)}>{tool.resultSummary}</span>
                      <span {...stylex.props(styles.toolTiming)}>{tool.timing}</span>
                    </span>
                  </button>

                  {open ? (
                    <div {...stylex.props(styles.jsonBlock)}>
                      <div {...stylex.props(styles.jsonBrace)}>{"{"}</div>
                      {tool.entries.map((entry, eIdx) => (
                        <div key={entry.key} {...stylex.props(styles.jsonLine)}>
                          <span {...stylex.props(styles.jsonKey)}>&quot;{entry.key}&quot;</span>
                          <span {...stylex.props(styles.jsonPunct)}>: </span>
                          <span
                            {...stylex.props(
                              entry.kind === "string" && styles.jsonString,
                              entry.kind === "number" && styles.jsonNumber,
                              entry.kind === "bool" && styles.jsonBool,
                            )}
                          >
                            {entry.value}
                          </span>
                          {eIdx < tool.entries.length - 1 ? (
                            <span {...stylex.props(styles.jsonPunct)}>,</span>
                          ) : null}
                        </div>
                      ))}
                      <div {...stylex.props(styles.jsonBrace)}>{"}"}</div>
                    </div>
                  ) : null}
                </div>
              </div>
            );
          })}
        </div>

        {/* Agent answer + live metrics + interactive action */}
        <div {...stylex.props(styles.answerCard, styles.lineReveal, dynamic.delay("130ms"))}>
          <div {...stylex.props(styles.metricRow)}>
            {activeMetrics.map((m) => (
              <span
                key={m.label}
                {...stylex.props(
                  styles.metricPill,
                  m.tone === "ok" && styles.metricPillOk,
                  m.tone === "accent" && styles.metricPillAccent,
                )}
              >
                {m.label}
              </span>
            ))}
          </div>

          <p {...stylex.props(styles.answerText)}>
            {acted ? scenario.actionDoneReply : scenario.reply}
          </p>

          {!acted ? <p {...stylex.props(styles.answerFollowUp)}>{scenario.followUp}</p> : null}

          <div {...stylex.props(styles.answerActions)}>
            <button
              type="button"
              onClick={() =>
                setActedIds((prev) => ({ ...prev, [scenario.id]: !prev[scenario.id] }))
              }
              {...stylex.props(styles.actionBtn, acted && styles.actionBtnDone)}
            >
              {acted ? (
                <Check size={14} aria-hidden="true" />
              ) : (
                <Wrench size={14} aria-hidden="true" />
              )}
              <span>{acted ? scenario.actionDoneLabel : scenario.actionLabel}</span>
            </button>
          </div>
        </div>
      </div>

      {/* Quiet bottom capability strip inside the MCP window */}
      <div {...stylex.props(styles.toolsetBar)}>
        <span {...stylex.props(styles.toolsetLabel)}>Exposed tools</span>
        <div {...stylex.props(styles.toolsetList)}>
          {MCP_TOOLS_LIST.map((tool) => {
            const activeTool = scenario.usedTools.includes(tool);
            return (
              <span
                key={tool}
                {...stylex.props(styles.toolsetChip, activeTool && styles.toolsetChipOn)}
              >
                {tool}
              </span>
            );
          })}
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// Styles
// ============================================================================
const HOVER = "@media (hover: hover) and (pointer: fine)";
const REDUCED = "@media (prefers-reduced-motion: reduce)";
const MOBILE = "@media (max-width: 680px)";
const PRESS_EASE = "cubic-bezier(0.25, 0.46, 0.45, 0.94)";

const lineIn = stylex.keyframes({
  from: { opacity: 0, transform: "translateY(4px)" },
  to: { opacity: 1, transform: "translateY(0)" },
});

const dynamic = stylex.create({
  delay: (animationDelay: string) => ({ animationDelay }),
  width: (width: string) => ({ width }),
});

const styles = stylex.create({
  lineReveal: {
    animationDuration: "180ms",
    animationFillMode: "both",
    animationName: { default: lineIn, [REDUCED]: "none" },
    animationTimingFunction: "cubic-bezier(0.25, 1, 0.5, 1)",
  },
  window: {
    borderRadius: 14,
    boxShadow:
      "0 0 0 1px oklch(1 0 0 / .11), 0 2px 6px oklch(0 0 0 / .28), 0 32px 84px oklch(0 0 0 / .52)",
    display: "flex",
    flexDirection: "column",
    marginInline: "auto",
    maxWidth: 1240,
    overflow: "hidden",
    width: "100%",
  },
  cliWindow: {
    backgroundColor: "oklch(0.125 0.014 286)",
    color: "oklch(0.93 0.01 290)",
  },
  mcpWindow: {
    backgroundColor: "oklch(0.135 0.017 290)",
    color: "oklch(0.93 0.01 290)",
  },
  titlebar: {
    alignItems: "center",
    backgroundColor: "oklch(1 0 0 / .025)",
    borderBottomColor: "oklch(1 0 0 / .08)",
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    display: "flex",
    gap: 12,
    justifyContent: "space-between",
    minHeight: 44,
    paddingInline: 16,
  },
  lights: {
    display: "flex",
    gap: 7,
    [MOBILE]: { display: "none" },
  },
  light: {
    backgroundColor: "oklch(1 0 0 / .18)",
    borderRadius: 999,
    height: 11,
    width: 11,
  },
  titlebarText: {
    color: "oklch(0.76 0.015 290)",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: 12,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  replayBtn: {
    alignItems: "center",
    backgroundColor: "transparent",
    borderRadius: 7,
    borderWidth: 0,
    color: "oklch(0.78 0.02 290)",
    cursor: "pointer",
    display: "inline-flex",
    fontFamily: "inherit",
    fontSize: 12.5,
    fontWeight: 540,
    gap: 6,
    minHeight: 32,
    paddingInline: 10,
    touchAction: "manipulation",
    transition: `background-color 140ms ease, color 140ms ease, transform 140ms ${PRESS_EASE}`,
    [HOVER]: { ":hover": { backgroundColor: "oklch(1 0 0 / .08)", color: "#fff" } },
    ":active": { transform: "scale(.97)" },
    ":focus-visible": { outline: "2px solid oklch(0.97 0 0)", outlineOffset: 1 },
  },
  subHeader: {
    alignItems: "center",
    backgroundColor: "oklch(1 0 0 / .018)",
    borderBottomColor: "oklch(1 0 0 / .07)",
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    display: "flex",
    flexWrap: "wrap",
    gap: 10,
    justifyContent: "space-between",
    padding: "9px 16px",
  },
  cmdStrip: {
    display: "flex",
    flexWrap: "wrap",
    gap: 4,
  },
  cmdTab: {
    alignItems: "center",
    backgroundColor: "transparent",
    borderRadius: 8,
    borderWidth: 0,
    color: "oklch(0.72 0.018 290)",
    cursor: "pointer",
    display: "inline-flex",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: 12.5,
    fontWeight: 540,
    gap: 6,
    minHeight: 34,
    paddingInline: 11,
    touchAction: "manipulation",
    transition: `background-color 140ms ease, color 140ms ease, transform 140ms ${PRESS_EASE}`,
    [HOVER]: { ":hover": { backgroundColor: "oklch(1 0 0 / .06)", color: "#fff" } },
    ":active": { transform: "scale(.97)" },
    ":focus-visible": { outline: "2px solid oklch(0.97 0 0)", outlineOffset: 1 },
  },
  cmdTabOn: {
    backgroundColor: "oklch(1 0 0 / .11)",
    boxShadow: "inset 0 0 0 1px oklch(1 0 0 / .14)",
    color: "#fff",
  },
  cmdTabPrefix: { color: "oklch(0.56 0.02 290)" },
  cmdTabPrefixOn: { color: "oklch(0.82 0.14 293)" },
  metaBadge: {
    alignItems: "center",
    color: "oklch(0.7 0.02 290)",
    display: "inline-flex",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: 11.5,
    fontVariantNumeric: "tabular-nums",
    gap: 7,
    whiteSpace: "nowrap",
  },
  greenDot: {
    backgroundColor: "oklch(0.75 0.17 150)",
    borderRadius: 999,
    height: 7,
    width: 7,
  },
  hintCaption: {
    color: "oklch(0.62 0.02 290)",
    fontSize: 12,
    [MOBILE]: { display: "none" },
  },

  // CLI body
  termBody: {
    display: "flex",
    flexDirection: "column",
    gap: 16,
    padding: "20px 22px 22px",
    [MOBILE]: { padding: "16px 14px 18px" },
  },
  promptRow: {
    alignItems: "center",
    display: "flex",
    flexWrap: "wrap",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: 13.5,
    gap: 8,
    lineHeight: 1.5,
  },
  promptPath: { color: "oklch(0.76 0.12 293)", fontWeight: 560 },
  promptBranch: {
    backgroundColor: "oklch(1 0 0 / .07)",
    borderRadius: 5,
    color: "oklch(0.74 0.02 290)",
    fontSize: 11.5,
    padding: "1px 7px",
  },
  promptCaret: { color: "oklch(0.75 0.17 150)", fontWeight: 700 },
  promptCmd: { color: "#fff", fontWeight: 600 },
  termSection: {
    display: "flex",
    flexDirection: "column",
    gap: 14,
  },
  budgetCard: {
    backgroundColor: "oklch(1 0 0 / .035)",
    borderColor: "oklch(1 0 0 / .08)",
    borderRadius: 10,
    borderStyle: "solid",
    borderWidth: 1,
    display: "flex",
    flexDirection: "column",
    gap: 10,
    padding: "12px 14px",
  },
  budgetHeader: {
    alignItems: "center",
    display: "flex",
    flexWrap: "wrap",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: 12,
    gap: 8,
    justifyContent: "space-between",
  },
  budgetTitle: { color: "oklch(0.86 0.015 290)" },
  budgetRight: { color: "oklch(0.66 0.02 290)", fontVariantNumeric: "tabular-nums" },
  budgetBar: {
    backgroundColor: "oklch(1 0 0 / .08)",
    borderRadius: 999,
    display: "flex",
    gap: 2,
    height: 8,
    overflow: "hidden",
  },
  segDescribed: { backgroundColor: "oklch(0.75 0.17 150)", width: "16%" },
  segNameOnly: { backgroundColor: "oklch(0.82 0.14 78)", width: "39%" },
  segHidden: { backgroundColor: "oklch(0.7 0.18 24)", width: "45%" },
  budgetLegend: {
    display: "flex",
    flexWrap: "wrap",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: 12,
    fontVariantNumeric: "tabular-nums",
    gap: 16,
  },
  legendItem: {
    alignItems: "center",
    color: "oklch(0.82 0.015 290)",
    display: "inline-flex",
    gap: 7,
  },
  legendSwatch: { borderRadius: 2, height: 8, width: 8 },
  swatchOk: { backgroundColor: "oklch(0.75 0.17 150)" },
  swatchWarn: { backgroundColor: "oklch(0.82 0.14 78)" },
  swatchErr: { backgroundColor: "oklch(0.7 0.18 24)" },
  swatchMuted: { backgroundColor: "oklch(0.45 0.02 290)" },

  cliTable: {
    borderColor: "oklch(1 0 0 / .08)",
    borderRadius: 10,
    borderStyle: "solid",
    borderWidth: 1,
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: 12.5,
    overflow: "hidden",
  },
  cliTableHead: {
    backgroundColor: "oklch(1 0 0 / .03)",
    borderBottomColor: "oklch(1 0 0 / .08)",
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    color: "oklch(0.6 0.02 290)",
    display: "grid",
    fontSize: 11,
    gap: 12,
    gridTemplateColumns: "92px minmax(0, 1fr) 168px",
    letterSpacing: "0.04em",
    padding: "8px 14px",
    [MOBILE]: { gap: 8, gridTemplateColumns: "78px minmax(0, 1fr) auto", padding: "8px 10px" },
  },
  cliTableRow: {
    alignItems: "center",
    borderBottomColor: "oklch(1 0 0 / .055)",
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    display: "grid",
    gap: 12,
    gridTemplateColumns: "92px minmax(0, 1fr) 168px",
    padding: "9px 14px",
    [MOBILE]: { gap: 8, gridTemplateColumns: "78px minmax(0, 1fr) auto", padding: "9px 10px" },
  },
  stateTag: {
    borderRadius: 5,
    fontSize: 11.5,
    fontWeight: 580,
    justifySelf: "start",
    padding: "2px 7px",
    whiteSpace: "nowrap",
  },
  stateTagOk: {
    backgroundColor: "oklch(0.75 0.17 150 / .14)",
    color: "oklch(0.78 0.16 150)",
  },
  stateTagWarn: {
    backgroundColor: "oklch(0.82 0.14 78 / .14)",
    color: "oklch(0.85 0.13 78)",
  },
  stateTagErr: {
    backgroundColor: "oklch(0.7 0.18 24 / .14)",
    color: "oklch(0.75 0.17 24)",
  },
  skillNameCell: {
    color: "oklch(0.95 0.005 290)",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  weightCell: {
    alignItems: "center",
    display: "flex",
    gap: 10,
    justifyContent: "flex-end",
  },
  miniBarTrack: {
    backgroundColor: "oklch(1 0 0 / .08)",
    borderRadius: 999,
    height: 5,
    overflow: "hidden",
    width: 64,
    [MOBILE]: { display: "none" },
  },
  miniBarFill: {
    borderRadius: 999,
    display: "block",
    height: "100%",
  },
  charsNum: {
    color: "oklch(0.74 0.02 290)",
    fontVariantNumeric: "tabular-nums",
    minWidth: { default: 74, [MOBILE]: "auto" },
    textAlign: "right",
    whiteSpace: "nowrap",
  },
  alignRight: { textAlign: "right" },
  hideMobile: { [MOBILE]: { display: "none" } },
  nextTipBtn: {
    alignItems: "center",
    backgroundColor: "oklch(0.72 0.16 293 / .1)",
    borderColor: {
      default: "oklch(0.72 0.16 293 / .26)",
      [HOVER]: { ":hover": "oklch(0.72 0.16 293 / .5)" },
    },
    borderRadius: 9,
    borderStyle: "solid",
    borderWidth: 1,
    color: "oklch(0.88 0.04 293)",
    cursor: "pointer",
    display: "flex",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: 12.5,
    gap: 9,
    lineHeight: 1.45,
    minHeight: 40,
    padding: "9px 13px",
    textAlign: "left",
    touchAction: "manipulation",
    transition: `background-color 140ms ease, border-color 140ms ease, transform 140ms ${PRESS_EASE}`,
    [HOVER]: { ":hover": { backgroundColor: "oklch(0.72 0.16 293 / .17)" } },
    ":active": { transform: "scale(.99)" },
    ":focus-visible": { outline: "2px solid oklch(0.97 0 0)", outlineOffset: 2 },
  },
  tipStrong: { color: "#fff", fontWeight: 620 },

  // Sessions & Usage
  statRibbon: {
    display: "flex",
    flexWrap: "wrap",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: 12,
    fontVariantNumeric: "tabular-nums",
    gap: 8,
  },
  statPill: {
    backgroundColor: "oklch(1 0 0 / .07)",
    borderRadius: 6,
    color: "oklch(0.9 0.01 290)",
    padding: "4px 10px",
  },
  statPillOk: {
    backgroundColor: "oklch(0.75 0.17 150 / .14)",
    borderRadius: 6,
    color: "oklch(0.79 0.16 150)",
    padding: "4px 10px",
  },
  statPillWarn: {
    backgroundColor: "oklch(0.82 0.14 78 / .14)",
    borderRadius: 6,
    color: "oklch(0.85 0.13 78)",
    padding: "4px 10px",
  },
  statPillMuted: {
    backgroundColor: "oklch(1 0 0 / .045)",
    borderRadius: 6,
    color: "oklch(0.72 0.02 290)",
    padding: "4px 10px",
  },
  cliSessionHead: {
    backgroundColor: "oklch(1 0 0 / .03)",
    borderBottomColor: "oklch(1 0 0 / .08)",
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    color: "oklch(0.6 0.02 290)",
    display: "grid",
    fontSize: 11,
    gap: 12,
    gridTemplateColumns: "minmax(0, 1fr) 96px 76px 136px",
    letterSpacing: "0.04em",
    padding: "8px 14px",
    [MOBILE]: { gridTemplateColumns: "minmax(0, 1fr) 82px 104px" },
  },
  cliSessionRow: {
    alignItems: "center",
    borderBottomColor: "oklch(1 0 0 / .055)",
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    display: "grid",
    fontVariantNumeric: "tabular-nums",
    gap: 12,
    gridTemplateColumns: "minmax(0, 1fr) 96px 76px 136px",
    padding: "9px 14px",
    [MOBILE]: { gridTemplateColumns: "minmax(0, 1fr) 82px 104px" },
  },
  mutedMono: { color: "oklch(0.66 0.02 290)" },
  toneOk: { color: "oklch(0.78 0.16 150)" },
  toneWarn: { color: "oklch(0.85 0.13 78)" },
  toneErr: { color: "oklch(0.75 0.17 24)" },
  toneAccent: { color: "oklch(0.84 0.14 293)" },

  // Diagnose
  diagList: { display: "flex", flexDirection: "column", gap: 10 },
  diagCard: {
    backgroundColor: "oklch(1 0 0 / .03)",
    borderLeftStyle: "solid",
    borderLeftWidth: 3,
    borderRadius: 9,
    display: "flex",
    flexDirection: "column",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: 12.5,
    gap: 5,
    padding: "11px 14px",
  },
  diagCardErr: { borderLeftColor: "oklch(0.72 0.18 24)" },
  diagCardWarn: { borderLeftColor: "oklch(0.82 0.14 78)" },
  diagTop: { display: "flex", flexWrap: "wrap", gap: 8 },
  diagLevel: { fontWeight: 640 },
  diagTitle: { color: "#fff" },
  diagPath: {
    color: "oklch(0.68 0.02 290)",
    fontSize: 12,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  diagHelp: { color: "oklch(0.78 0.13 150)", fontSize: 12 },
  diagFoot: {
    color: "oklch(0.72 0.02 290)",
    display: "flex",
    flexWrap: "wrap",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: 12.5,
    gap: 10,
    justifyContent: "space-between",
    paddingTop: 2,
  },

  // Usage
  usageList: {
    borderColor: "oklch(1 0 0 / .08)",
    borderRadius: 10,
    borderStyle: "solid",
    borderWidth: 1,
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: 12.5,
    overflow: "hidden",
  },
  usageRow: {
    alignItems: "center",
    borderBottomColor: "oklch(1 0 0 / .055)",
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    display: "grid",
    fontVariantNumeric: "tabular-nums",
    gap: 14,
    gridTemplateColumns: "180px minmax(60px, 1fr) 74px 96px",
    padding: "10px 14px",
    [MOBILE]: { gridTemplateColumns: "minmax(0, 1fr) 64px 82px" },
  },
  usageBarWrap: {
    backgroundColor: "oklch(1 0 0 / .07)",
    borderRadius: 999,
    height: 7,
    overflow: "hidden",
    [MOBILE]: { display: "none" },
  },
  usageBarFill: { borderRadius: 999, display: "block", height: "100%" },
  usageCount: { color: "#fff", textAlign: "right" },
  usageLast: { color: "oklch(0.66 0.02 290)", textAlign: "right" },

  // Park
  parkSuccessCard: {
    backgroundColor: "oklch(0.75 0.17 150 / .09)",
    borderColor: "oklch(0.75 0.17 150 / .28)",
    borderRadius: 10,
    borderStyle: "solid",
    borderWidth: 1,
    display: "flex",
    flexDirection: "column",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    gap: 6,
    padding: "13px 15px",
  },
  parkBanner: {
    alignItems: "center",
    color: "oklch(0.8 0.16 150)",
    display: "flex",
    fontSize: 13,
    fontWeight: 620,
    gap: 8,
  },
  parkList: { color: "oklch(0.78 0.02 290)", fontSize: 12, lineHeight: 1.5 },
  deltaGrid: {
    display: "grid",
    gap: 10,
    gridTemplateColumns: "repeat(3, minmax(0, 1fr))",
    [MOBILE]: { gridTemplateColumns: "1fr" },
  },
  deltaCard: {
    backgroundColor: "oklch(1 0 0 / .035)",
    borderColor: "oklch(1 0 0 / .08)",
    borderRadius: 10,
    borderStyle: "solid",
    borderWidth: 1,
    display: "flex",
    flexDirection: "column",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    gap: 4,
    padding: "13px 14px",
  },
  deltaLabel: { color: "oklch(0.66 0.02 290)", fontSize: 11.5 },
  deltaValue: { color: "#fff", fontSize: 20, fontVariantNumeric: "tabular-nums", fontWeight: 680 },
  deltaNote: { color: "oklch(0.72 0.02 290)", fontSize: 11.5 },

  // MCP specific
  mcpTitleLeft: { alignItems: "center", display: "flex", gap: 14, minWidth: 0 },
  agentBadge: {
    alignItems: "center",
    color: "#fff",
    display: "inline-flex",
    fontSize: 12.5,
    fontWeight: 580,
    gap: 7,
    overflow: "hidden",
    whiteSpace: "nowrap",
  },
  agentSep: { color: "oklch(0.5 0.02 290)", [MOBILE]: { display: "none" } },
  agentServer: {
    color: "oklch(0.78 0.08 293)",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: 12,
    [MOBILE]: { display: "none" },
  },
  mcpBody: {
    display: "flex",
    flexDirection: "column",
    gap: 16,
    padding: "20px 22px 20px",
    [MOBILE]: { padding: "16px 14px" },
  },
  mcpPromptCard: {
    alignItems: "baseline",
    backgroundColor: "oklch(1 0 0 / .045)",
    borderColor: "oklch(1 0 0 / .09)",
    borderRadius: 11,
    borderStyle: "solid",
    borderWidth: 1,
    display: "flex",
    gap: 10,
    padding: "13px 16px",
  },
  mcpPromptChevron: {
    color: "oklch(0.78 0.14 293)",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: 16,
    fontWeight: 700,
    lineHeight: 1,
  },
  mcpPromptText: {
    color: "#fff",
    fontSize: 14.5,
    fontWeight: 520,
    lineHeight: 1.5,
  },
  timeline: {
    borderLeftColor: "oklch(1 0 0 / .1)",
    borderLeftStyle: "solid",
    borderLeftWidth: 1,
    display: "flex",
    flexDirection: "column",
    gap: 10,
    marginLeft: 10,
    paddingLeft: 18,
  },
  toolNode: {
    position: "relative",
  },
  timelineDot: {
    backgroundColor: "oklch(0.76 0.15 293)",
    borderRadius: 999,
    boxShadow: "0 0 0 3px oklch(0.135 0.017 290)",
    height: 7,
    left: -22,
    position: "absolute",
    top: 17,
    width: 7,
  },
  toolCard: {
    backgroundColor: "oklch(0.105 0.014 288)",
    borderColor: "oklch(1 0 0 / .09)",
    borderRadius: 10,
    borderStyle: "solid",
    borderWidth: 1,
    overflow: "hidden",
  },
  toolRowBtn: {
    alignItems: "center",
    backgroundColor: "transparent",
    borderWidth: 0,
    color: "inherit",
    cursor: "pointer",
    display: "flex",
    flexWrap: "wrap",
    fontFamily: "inherit",
    gap: 10,
    justifyContent: "space-between",
    minHeight: 42,
    padding: "8px 13px",
    textAlign: "left",
    touchAction: "manipulation",
    width: "100%",
    [HOVER]: { ":hover": { backgroundColor: "oklch(1 0 0 / .04)" } },
    ":focus-visible": { outline: "2px solid oklch(0.97 0 0)", outlineOffset: -2 },
  },
  toolHeadLeft: {
    alignItems: "center",
    color: "oklch(0.82 0.13 293)",
    display: "inline-flex",
    flexWrap: "wrap",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: 12.5,
    fontWeight: 560,
    gap: 7,
  },
  toolName: { whiteSpace: "nowrap" },
  toolArgs: {
    backgroundColor: "oklch(1 0 0 / .06)",
    borderRadius: 5,
    color: "oklch(0.74 0.02 290)",
    fontSize: 11.5,
    fontWeight: 450,
    padding: "2px 7px",
  },
  toolHeadRight: {
    alignItems: "center",
    display: "inline-flex",
    gap: 10,
  },
  toolSummary: {
    color: "oklch(0.8 0.015 290)",
    fontSize: 12.5,
    fontVariantNumeric: "tabular-nums",
  },
  toolTiming: {
    color: "oklch(0.58 0.02 290)",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: 11.5,
    fontVariantNumeric: "tabular-nums",
  },
  jsonBlock: {
    backgroundColor: "oklch(0.09 0.012 288)",
    borderTopColor: "oklch(1 0 0 / .07)",
    borderTopStyle: "solid",
    borderTopWidth: 1,
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: 12,
    lineHeight: 1.65,
    overflowX: "auto",
    padding: "10px 16px",
  },
  jsonBrace: { color: "oklch(0.6 0.02 290)" },
  jsonLine: { paddingLeft: 16, whiteSpace: "nowrap" },
  jsonKey: { color: "oklch(0.8 0.12 293)" },
  jsonPunct: { color: "oklch(0.6 0.02 290)" },
  jsonString: { color: "oklch(0.78 0.15 150)" },
  jsonNumber: { color: "oklch(0.84 0.14 78)" },
  jsonBool: { color: "oklch(0.76 0.14 230)" },

  answerCard: {
    backgroundColor: "oklch(1 0 0 / .03)",
    borderColor: "oklch(1 0 0 / .08)",
    borderRadius: 12,
    borderStyle: "solid",
    borderWidth: 1,
    display: "flex",
    flexDirection: "column",
    gap: 11,
    padding: "15px 17px",
  },
  metricRow: { display: "flex", flexWrap: "wrap", gap: 7 },
  metricPill: {
    backgroundColor: "oklch(1 0 0 / .07)",
    borderRadius: 6,
    color: "oklch(0.82 0.015 290)",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: 11.5,
    fontVariantNumeric: "tabular-nums",
    fontWeight: 560,
    padding: "3px 9px",
  },
  metricPillOk: {
    backgroundColor: "oklch(0.75 0.17 150 / .15)",
    color: "oklch(0.8 0.16 150)",
  },
  metricPillAccent: {
    backgroundColor: "oklch(0.72 0.16 293 / .18)",
    color: "oklch(0.88 0.1 293)",
  },
  answerText: {
    color: "oklch(0.95 0.005 290)",
    fontSize: 14,
    lineHeight: 1.6,
    margin: 0,
  },
  answerFollowUp: {
    color: "oklch(0.72 0.02 290)",
    fontSize: 13,
    lineHeight: 1.5,
    margin: 0,
  },
  answerActions: { display: "flex", gap: 8, marginTop: 2 },
  actionBtn: {
    alignItems: "center",
    backgroundColor: "oklch(0.72 0.16 293 / .18)",
    borderColor: "oklch(0.72 0.16 293 / .42)",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    color: "oklch(0.94 0.06 293)",
    cursor: "pointer",
    display: "inline-flex",
    fontFamily: "inherit",
    fontSize: 13,
    fontWeight: 580,
    gap: 7,
    minHeight: 36,
    paddingInline: 13,
    touchAction: "manipulation",
    transition: `background-color 140ms ease, border-color 140ms ease, color 140ms ease, transform 140ms ${PRESS_EASE}`,
    [HOVER]: { ":hover": { backgroundColor: "oklch(0.72 0.16 293 / .28)" } },
    ":active": { transform: "scale(.97)" },
    ":focus-visible": { outline: "2px solid currentColor", outlineOffset: 2 },
  },
  actionBtnDone: {
    backgroundColor: "oklch(0.75 0.17 150 / .16)",
    borderColor: siteTokens.success,
    color: siteTokens.success,
  },

  toolsetBar: {
    alignItems: "center",
    backgroundColor: "oklch(1 0 0 / .02)",
    borderTopColor: "oklch(1 0 0 / .07)",
    borderTopStyle: "solid",
    borderTopWidth: 1,
    display: "flex",
    flexWrap: "wrap",
    gap: 10,
    padding: "10px 18px",
  },
  toolsetLabel: {
    color: "oklch(0.58 0.02 290)",
    fontSize: 11.5,
    fontWeight: 560,
    whiteSpace: "nowrap",
  },
  toolsetList: { display: "flex", flexWrap: "wrap", gap: 6 },
  toolsetChip: {
    backgroundColor: "oklch(1 0 0 / .04)",
    borderRadius: 5,
    color: "oklch(0.64 0.02 290)",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: 11,
    padding: "2px 8px",
  },
  toolsetChipOn: {
    backgroundColor: "oklch(0.72 0.16 293 / .18)",
    color: "oklch(0.88 0.1 293)",
  },
});
