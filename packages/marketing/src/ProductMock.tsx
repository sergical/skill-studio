// ============================================================================
// Skill Studio marketing demo - interactive preview of the desktop product
// Mirrors the desktop shell (apps/desktop/src/App.tsx), the sidebar, the page
// header (PageShell), and the shared skill row (components/SkillList).
// ============================================================================

import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
} from "react";
import * as stylex from "@stylexjs/stylex";
import {
  Activity,
  BookOpen,
  Check,
  ChevronDown,
  ChevronRight,
  CircleAlert,
  CircleArrowDown,
  CirclePause,
  Folder,
  FolderGit2,
  GitFork,
  Globe,
  Info,
  Layers,
  LayoutDashboard,
  LayoutGrid,
  Link2,
  Link2Off,
  List,
  ListFilter,
  Moon,
  MoreHorizontal,
  OctagonAlert,
  PackageOpen,
  Plus,
  Puzzle,
  RefreshCw,
  Search,
  Settings,
  Sun,
  TriangleAlert,
  X,
} from "lucide-react";

import { AgentIcon, type AgentId } from "./AgentIcon";
import { productDemoLightTheme, productDemoTokens as tokens } from "./ProductDemoTokens.stylex";
import type { SiteTheme } from "./SiteTheme.stylex";
import { productMockDetailBackLabel, type ProductMockRootView } from "./product-mock-navigation";

export interface ThemeToggleOrigin {
  x: number;
  y: number;
}

interface ProductMockProps {
  theme: SiteTheme;
  onToggleTheme: (origin: ThemeToggleOrigin) => void;
}

type Harness = AgentId | "universal";
type SkillLocation = "Global" | "agent-studio";
type RowState = "broken" | "warning" | "update" | "parked";
type FixLabel = "Fix" | "Compare" | "Update" | "Unpark";
type Scope = "All" | "Global" | "Project";
type ActivityWindow = "24h" | "7d" | "14d" | "30d";
type InvocationPolicy = "Both" | "User only" | "Model only";
type HomeFilter = "broken" | "warning" | "update" | null;
type InstallMethod = "skills.sh" | "dotagents" | "Copy";

interface SkillRow {
  name: string;
  description: string;
  source: string;
  locations: readonly SkillLocation[];
  harnesses: readonly Harness[];
  claudeLink?: "linked" | "broken";
  state?: RowState;
  specViolation?: boolean;
  note?: string;
  fix?: FixLabel;
  method?: InstallMethod;
  promptTokens: number;
  fullTokens: string;
}

interface DetailState {
  skill: SkillRow;
  from: ProductMockRootView;
}

const skills: readonly SkillRow[] = [
  {
    name: "motion",
    description: "Animate React interfaces with Motion.",
    source: "emilkowalski/skills",
    locations: ["Global"],
    harnesses: ["universal", "claude"],
    claudeLink: "broken",
    state: "broken",
    note: "Claude Code · broken link at ~/.claude/skills/motion",
    fix: "Fix",
    promptTokens: 11,
    fullTokens: "1.4k",
  },
  {
    name: "prepare-videos",
    description: "Prepare product videos for upload.",
    source: "sergical/skills",
    locations: ["Global"],
    harnesses: ["universal"],
    state: "broken",
    specViolation: true,
    note: "SKILL.md has no description in its frontmatter",
    fix: "Fix",
    promptTokens: 6,
    fullTokens: "980",
  },
  {
    name: "tailwind",
    description: "Write Tailwind CSS 4 utility classes.",
    source: "tailwindlabs/skills",
    locations: ["Global", "agent-studio"],
    harnesses: ["universal", "claude"],
    claudeLink: "linked",
    state: "broken",
    specViolation: true,
    note: "SKILL.md has no name in its frontmatter",
    fix: "Fix",
    promptTokens: 8,
    fullTokens: "2.1k",
  },
  {
    name: "writing-skills",
    description: "Write and test new agent skills.",
    source: "obra/superpowers",
    locations: ["Global"],
    harnesses: ["universal"],
    state: "broken",
    specViolation: true,
    note: "Name does not match its directory",
    fix: "Fix",
    promptTokens: 12,
    fullTokens: "1.6k",
  },
  {
    name: "commit",
    description: "Create commits that follow repository conventions.",
    source: "sergical/skills",
    locations: ["Global", "agent-studio"],
    harnesses: ["universal", "claude", "codex"],
    claudeLink: "linked",
    state: "warning",
    note: "3 copies differ: Global · Universal folder · agent-studio",
    fix: "Compare",
    promptTokens: 18,
    fullTokens: "1.1k",
  },
  {
    name: "add-customer-story",
    description: "Add a customer story page to the site.",
    source: "sergical/skills",
    locations: ["Global"],
    harnesses: ["universal", "claude"],
    state: "warning",
    note: "2 copies differ: Universal folder · Claude Code",
    fix: "Compare",
    promptTokens: 14,
    fullTokens: "1.7k",
  },
  {
    name: "code-simplifier",
    description: "Simplify code without changing what it does.",
    source: "anthropics/skills",
    locations: ["Global"],
    harnesses: ["universal", "claude", "codex"],
    state: "warning",
    note: "4 copies differ: Universal folder · Claude Code · Codex",
    fix: "Compare",
    promptTokens: 10,
    fullTokens: "890",
  },
  {
    name: "agent-browser",
    description: "Automate a browser from the terminal.",
    source: "vercel-labs/agent-browser",
    locations: ["Global"],
    harnesses: ["universal", "claude", "codex", "opencode", "cursor"],
    claudeLink: "linked",
    state: "update",
    note: "716c069 → 021d925",
    fix: "Update",
    promptTokens: 18,
    fullTokens: "2.4k",
  },
  {
    name: "cmux",
    description: "Manage cmux workspaces, panes, and terminals.",
    source: "manaflow-ai/cmux",
    locations: ["Global"],
    harnesses: ["universal", "claude"],
    claudeLink: "linked",
    state: "update",
    note: "652d0c8 → 1349bc0",
    fix: "Update",
    promptTokens: 9,
    fullTokens: "1.3k",
  },
  {
    name: "docker-multistage",
    description: "Convert a Dockerfile to a leaner multi-stage build.",
    source: "docker/buildx",
    locations: ["Global"],
    harnesses: ["universal", "claude"],
    claudeLink: "linked",
    state: "update",
    note: "v1 → abc1234",
    fix: "Update",
    promptTokens: 9,
    fullTokens: "560",
  },
  {
    name: "backport-pr",
    description: "Backport a merged PR to a maintenance branch.",
    source: "getsentry/skills",
    locations: ["Global"],
    harnesses: ["universal", "claude"],
    claudeLink: "linked",
    promptTokens: 12,
    fullTokens: "2.7k",
  },
  {
    name: "code-review",
    description: "Review changes against standards and the spec.",
    source: "getsentry/skills",
    locations: ["Global"],
    harnesses: ["universal", "claude", "codex"],
    claudeLink: "linked",
    promptTokens: 18,
    fullTokens: "1.5k",
  },
  {
    name: "design-vocabulary",
    description: "Precise language for interface and product design.",
    source: "emilkowalski/skills",
    locations: ["Global"],
    harnesses: ["universal", "claude", "codex", "opencode", "pi", "cursor"],
    claudeLink: "linked",
    promptTokens: 11,
    fullTokens: "1.5k",
  },
  {
    name: "new-branch",
    description: "Create a branch from the latest default branch.",
    source: "sergical/skills",
    locations: ["agent-studio"],
    harnesses: ["claude"],
    promptTokens: 7,
    fullTokens: "792",
  },
  {
    name: "pstack-technical-writing",
    description: "Layered standards for clear technical writing.",
    source: "pstack/skills",
    locations: ["agent-studio"],
    harnesses: ["universal", "claude"],
    claudeLink: "linked",
    promptTokens: 10,
    fullTokens: "2.7k",
  },
  {
    name: "sentry-cli",
    description: "Query Sentry issues and releases from the terminal.",
    source: "getsentry/cli",
    locations: ["Global"],
    harnesses: ["universal", "claude", "codex"],
    claudeLink: "linked",
    promptTokens: 9,
    fullTokens: "1.2k",
  },
];

const initialParked: readonly SkillRow[] = [
  {
    name: "tailwind-v3",
    description: "Tailwind CSS 3 class reference.",
    source: "tailwindlabs/skills",
    locations: ["Global"],
    harnesses: ["universal"],
    state: "parked",
    note: "Parked 3 days ago",
    fix: "Unpark",
    promptTokens: 9,
    fullTokens: "1.9k",
  },
  {
    name: "jira-sync",
    description: "Sync pull requests with Jira tickets.",
    source: "sergical/skills",
    locations: ["agent-studio"],
    harnesses: ["claude"],
    state: "parked",
    note: "Parked 2 weeks ago",
    fix: "Unpark",
    promptTokens: 13,
    fullTokens: "1.1k",
  },
  {
    name: "old-release-notes",
    description: "Draft release notes from merged PRs.",
    source: "sergical/skills",
    locations: ["Global"],
    harnesses: ["universal", "claude"],
    state: "parked",
    note: "Parked last month",
    fix: "Unpark",
    promptTokens: 8,
    fullTokens: "640",
  },
];

const SKILL_TOTAL = 290;

function skillNamed(name: string): SkillRow {
  return (
    skills.find((skill) => skill.name === name) ?? {
      name,
      description: "Reusable instructions for a repeated task.",
      source: "sergical/skills",
      locations: ["Global"],
      harnesses: ["universal", "claude"],
      claudeLink: "linked",
      promptTokens: 10,
      fullTokens: "1.2k",
    }
  );
}

const UPDATE_TOTAL = 22;

const homeGroups = [
  { state: "broken", label: "Broken", total: 5 },
  { state: "warning", label: "Warnings", total: 34 },
  { state: "update", label: "Updates", total: UPDATE_TOTAL },
] satisfies ReadonlyArray<{ state: Exclude<HomeFilter, null>; label: string; total: number }>;

const pluginGroups = [
  {
    name: "codex",
    version: "1.0.6",
    agent: "codex",
    skills: [
      ["codex-cli-runtime", "Call the Codex companion runtime.", "640"],
      ["codex-result-handling", "Present Codex helper output clearly.", "410"],
      ["gpt-5-4-prompting", "Compose focused coding and review prompts.", "1.2k"],
    ],
  },
  {
    name: "figma",
    version: "2.1.7",
    agent: "claude",
    skills: [
      ["figma-code-connect", "Map Figma components to code snippets.", "2.3k"],
      ["figma-create-design-system-rules", "Generate project design-system rules.", "1.8k"],
      ["figma-generate-design", "Translate application views into Figma.", "3.1k"],
    ],
  },
] satisfies ReadonlyArray<{
  name: string;
  version: string;
  agent: AgentId;
  skills: ReadonlyArray<readonly [string, string, string]>;
}>;

const navItems: ReadonlyArray<{
  id: ProductMockRootView;
  label: string;
  Icon: typeof LayoutDashboard;
  count?: number;
}> = [
  { id: "home", label: "Home", Icon: LayoutDashboard },
  { id: "skills", label: "Skills", Icon: Layers, count: SKILL_TOTAL },
  { id: "plugins", label: "Plugins", Icon: Puzzle, count: 109 },
  { id: "activity", label: "Activity", Icon: Activity },
];

const viewEnter = stylex.keyframes({
  from: { opacity: 0.35, transform: "translateX(7px)" },
  to: { opacity: 1, transform: "translateX(0)" },
});

function toggleThemeFromButton(
  event: MouseEvent<HTMLButtonElement>,
  onToggleTheme: (origin: ThemeToggleOrigin) => void,
) {
  const bounds = event.currentTarget.getBoundingClientRect();
  onToggleTheme({ x: bounds.left + bounds.width / 2, y: bounds.top + bounds.height / 2 });
}

function Sidebar({
  activeView,
  parkedCount,
  onNavigate,
  onAddSkill,
  theme,
  onToggleTheme,
}: {
  activeView: ProductMockRootView;
  parkedCount: number;
  onNavigate: (view: ProductMockRootView) => void;
  onAddSkill: () => void;
  theme: SiteTheme;
  onToggleTheme: (origin: ThemeToggleOrigin) => void;
}) {
  const navButton = ({ id, label, Icon, count }: (typeof navItems)[number]) => (
    <button
      key={id}
      type="button"
      aria-label={label}
      aria-current={activeView === id ? "page" : undefined}
      onClick={() => onNavigate(id)}
      {...stylex.props(styles.navItem, activeView === id && styles.navItemActive)}
    >
      <Icon aria-hidden="true" size={14} {...stylex.props(styles.navIcon)} />
      <span {...stylex.props(activeView !== id && styles.navLabelCollapsible)}>{label}</span>
      {count !== undefined && <small {...stylex.props(styles.navCount)}>{count}</small>}
    </button>
  );
  return (
    <aside {...stylex.props(styles.sidebar)}>
      <div {...stylex.props(styles.dragStrip)}>
        <span {...stylex.props(styles.traffic)} aria-hidden="true">
          <i {...stylex.props(styles.trafficDot)} />
          <i {...stylex.props(styles.trafficDot)} />
          <i {...stylex.props(styles.trafficDot)} />
        </span>
      </div>
      <div {...stylex.props(styles.sidebarTitleRow)}>
        <span {...stylex.props(styles.sidebarTitle)}>Skill Studio</span>
        <span {...stylex.props(styles.sidebarTitleActions)}>
          <button
            type="button"
            aria-label="Search skills"
            onClick={() => onNavigate("skills")}
            {...stylex.props(styles.iconButton)}
          >
            <Search aria-hidden="true" size={14} />
          </button>
          <button
            type="button"
            aria-label="Add skill"
            onClick={onAddSkill}
            {...stylex.props(styles.iconButton)}
          >
            <Plus aria-hidden="true" size={14} />
          </button>
        </span>
      </div>
      <nav {...stylex.props(styles.productNav)} aria-label="Product navigation">
        {navItems.map(navButton)}
        {parkedCount > 0 && (
          <span {...stylex.props(styles.navGap)}>
            {navButton({ id: "parked", label: "Parked", Icon: PackageOpen, count: parkedCount })}
          </span>
        )}
      </nav>
      <div {...stylex.props(styles.sidebarFooter)}>
        <button type="button" {...stylex.props(styles.syncButton)}>
          <RefreshCw aria-hidden="true" size={13} />
          Sync
        </button>
        <span {...stylex.props(styles.footerActions)}>
          <button
            type="button"
            aria-label="Learn"
            {...stylex.props(styles.iconButton, styles.footerOnlyDesktop)}
          >
            <BookOpen aria-hidden="true" size={13} />
          </button>
          <button
            type="button"
            aria-label="Settings"
            {...stylex.props(styles.iconButton, styles.footerOnlyDesktop)}
          >
            <Settings aria-hidden="true" size={13} />
          </button>
          <button
            type="button"
            aria-label={`Switch to ${theme === "dark" ? "light" : "dark"} theme`}
            onClick={(event) => toggleThemeFromButton(event, onToggleTheme)}
            {...stylex.props(styles.iconButton, styles.themeButton)}
          >
            {theme === "dark" ? (
              <Moon aria-hidden="true" size={13} />
            ) : (
              <Sun aria-hidden="true" size={13} />
            )}
          </button>
        </span>
      </div>
    </aside>
  );
}

function PageHeader({
  title,
  parent,
  onParent,
  subtitle,
  actions,
}: {
  title: string;
  parent?: string;
  onParent?: () => void;
  subtitle?: string;
  actions?: ReactNode;
}) {
  return (
    <header {...stylex.props(styles.pageHeader)}>
      <h2 {...stylex.props(styles.crumbs)}>
        {parent && (
          <>
            <button type="button" onClick={onParent} {...stylex.props(styles.crumbParent)}>
              {parent}
            </button>
            <ChevronRight aria-hidden="true" size={12} {...stylex.props(styles.crumbChevron)} />
          </>
        )}
        <span {...stylex.props(styles.crumbTitle)}>{title}</span>
      </h2>
      {subtitle && <span {...stylex.props(styles.crumbSubtitle)}>· {subtitle}</span>}
      {actions && <div {...stylex.props(styles.headerActions)}>{actions}</div>}
    </header>
  );
}

function StateGlyph({ state, specViolation }: { state?: RowState; specViolation?: boolean }) {
  switch (state) {
    case "broken":
      return specViolation ? (
        <OctagonAlert aria-label="Spec violation" size={14} {...stylex.props(styles.danger)} />
      ) : (
        <CircleAlert aria-label="Broken" size={14} {...stylex.props(styles.danger)} />
      );
    case "warning":
      return <TriangleAlert aria-label="Warning" size={14} {...stylex.props(styles.warning)} />;
    case "update":
      return (
        <CircleArrowDown aria-label="Update available" size={14} {...stylex.props(styles.accent)} />
      );
    case "parked":
      return <CirclePause aria-label="Parked" size={14} {...stylex.props(styles.faint)} />;
    case undefined:
      return <span />;
  }
}

function LocationCell({ locations }: { locations: readonly SkillLocation[] }) {
  const [first, ...rest] = locations;
  return (
    <span {...stylex.props(styles.locationCell)}>
      <span {...stylex.props(styles.locationChip)}>
        {first === "Global" ? (
          <Globe aria-hidden="true" size={13} {...stylex.props(styles.faint)} />
        ) : (
          <FolderGit2 aria-hidden="true" size={13} {...stylex.props(styles.faint)} />
        )}
        <span {...stylex.props(styles.truncate)}>{first}</span>
      </span>
      {rest.length > 0 && <span {...stylex.props(styles.faint)}>+{rest.length}</span>}
    </span>
  );
}

const MAX_DISCS = 5;

function HarnessDiscs({
  harnesses,
  claudeLink,
}: {
  harnesses: readonly Harness[];
  claudeLink?: SkillRow["claudeLink"];
}) {
  const shown = harnesses.slice(0, MAX_DISCS);
  const overflow = harnesses.length - shown.length;
  return (
    <span {...stylex.props(styles.discs)}>
      {shown.map((harness, index) => (
        <span key={harness} {...stylex.props(styles.disc, index > 0 && styles.discOverlap)}>
          {harness === "universal" ? (
            <Layers aria-label="Universal folder" size={11} />
          ) : (
            <AgentIcon agent={harness} size={11} />
          )}
          {harness === "claude" && claudeLink && (
            <span {...stylex.props(styles.discBadge)}>
              {claudeLink === "broken" ? (
                <Link2Off aria-label="Broken link" size={7} {...stylex.props(styles.danger)} />
              ) : (
                <Link2 aria-label="Linked" size={7} {...stylex.props(styles.muted)} />
              )}
            </span>
          )}
        </span>
      ))}
      {overflow > 0 && <span {...stylex.props(styles.discOverflow)}>+{overflow}</span>}
    </span>
  );
}

function SkillListRow({
  skill,
  onOpen,
  onFix,
}: {
  skill: SkillRow;
  onOpen: () => void;
  onFix: () => void;
}) {
  return (
    <div {...stylex.props(styles.row)}>
      <span {...stylex.props(styles.rowSlot)}>
        <StateGlyph state={skill.state} specViolation={skill.specViolation} />
      </span>
      <button type="button" onClick={onOpen} {...stylex.props(styles.rowName)}>
        <span {...stylex.props(styles.truncate)}>{skill.name}</span>
      </button>
      <LocationCell locations={skill.locations} />
      <span {...stylex.props(styles.hideBelowNarrow)}>
        <HarnessDiscs harnesses={skill.harnesses} claudeLink={skill.claudeLink} />
      </span>
      <span {...stylex.props(styles.noteCell)}>
        <span {...stylex.props(styles.noteText)}>{skill.note}</span>
        <span {...stylex.props(styles.tokenPair)}>
          <span {...stylex.props(styles.faint)}>{skill.promptTokens}</span>
          <span>{skill.fullTokens}</span>
        </span>
      </span>
      <span {...stylex.props(styles.fixCell)}>
        {skill.fix && (
          <button type="button" onClick={onFix} {...stylex.props(styles.fixButton)}>
            {skill.fix}
          </button>
        )}
      </span>
    </div>
  );
}

interface GroupAction {
  label: string;
  onClick: () => void;
}

function GroupHead({
  label,
  count,
  open,
  onToggle,
  action,
}: {
  label: string;
  count: number;
  open: boolean;
  onToggle: () => void;
  action?: GroupAction;
}) {
  return (
    <div {...stylex.props(styles.groupHead)}>
      <button
        type="button"
        aria-expanded={open}
        onClick={onToggle}
        {...stylex.props(styles.groupToggle)}
      >
        <ChevronDown
          aria-hidden="true"
          size={14}
          {...stylex.props(styles.groupChevron, !open && styles.groupChevronClosed)}
        />
        {label}
        <span {...stylex.props(styles.groupCount)}>{count}</span>
      </button>
      {action && (
        <button type="button" onClick={action.onClick} {...stylex.props(styles.groupAction)}>
          {action.label}
        </button>
      )}
    </div>
  );
}

function SkillGroup({
  label,
  count,
  rows,
  action,
  defaultOpen = true,
  onOpenSkill,
  onFix,
}: {
  label: string;
  count: number;
  rows: readonly SkillRow[];
  action?: GroupAction;
  defaultOpen?: boolean;
  onOpenSkill: (skill: SkillRow) => void;
  onFix: (skill: SkillRow) => void;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <section {...stylex.props(styles.group)}>
      <GroupHead
        label={label}
        count={count}
        open={open}
        onToggle={() => setOpen(!open)}
        action={action}
      />
      {open &&
        rows.map((skill) => (
          <SkillListRow
            key={skill.name}
            skill={skill}
            onOpen={() => onOpenSkill(skill)}
            onFix={() => onFix(skill)}
          />
        ))}
    </section>
  );
}

function HomeScreen({
  rows,
  updateTotal,
  onOpenSkill,
  onFix,
  onUpdateAll,
}: {
  rows: readonly SkillRow[];
  updateTotal: number;
  onOpenSkill: (skill: SkillRow) => void;
  onFix: (skill: SkillRow) => void;
  onUpdateAll: () => void;
}) {
  const [filter, setFilter] = useState<HomeFilter>(null);
  const toggleFilter = (next: Exclude<HomeFilter, null>) =>
    setFilter((current) => (current === next ? null : next));
  const groups = homeGroups.map((group) => ({
    ...group,
    total: group.state === "update" ? updateTotal : group.total,
    rows: rows.filter((skill) => skill.state === group.state),
  }));
  return (
    <>
      <PageHeader title="Home" />
      <div {...stylex.props(styles.view)}>
        <section {...stylex.props(styles.page)}>
          <div {...stylex.props(styles.statGrid)}>
            {groups.map((group) => (
              <div key={group.state} {...stylex.props(styles.statWrap)}>
                <button
                  type="button"
                  aria-pressed={filter === group.state}
                  onClick={() => toggleFilter(group.state)}
                  {...stylex.props(styles.statTile, filter === group.state && styles.statTileOn)}
                >
                  <span
                    {...stylex.props(styles.statLabel, filter === group.state && styles.accent)}
                  >
                    {group.label}
                  </span>
                  <span
                    {...stylex.props(
                      styles.statValue,
                      group.state === "broken" && styles.danger,
                      group.state === "warning" && styles.warning,
                    )}
                  >
                    {group.total}
                  </span>
                  {group.state === "update" && (
                    <span {...stylex.props(styles.statCaption)}>Checked just now</span>
                  )}
                </button>
                {group.state === "update" && (
                  <button
                    type="button"
                    aria-label="Check now"
                    {...stylex.props(styles.iconButton, styles.statCheck)}
                  >
                    <RefreshCw aria-hidden="true" size={14} />
                  </button>
                )}
              </div>
            ))}
          </div>
          <LaneCard />
          <div {...stylex.props(styles.groups)}>
            {groups
              .filter(
                (group) => group.rows.length > 0 && (filter === null || filter === group.state),
              )
              .map((group) => (
                <SkillGroup
                  key={group.state}
                  label={group.label}
                  count={group.total}
                  rows={group.rows}
                  action={
                    group.state === "update"
                      ? { label: "Update all", onClick: onUpdateAll }
                      : undefined
                  }
                  onOpenSkill={onOpenSkill}
                  onFix={onFix}
                />
              ))}
            {filter === null && (
              <SkillGroup
                label="Not used in the last 30 days"
                count={76}
                rows={skills.filter((skill) => !skill.state).slice(0, 4)}
                defaultOpen={false}
                onOpenSkill={onOpenSkill}
                onFix={onFix}
              />
            )}
          </div>
        </section>
      </div>
    </>
  );
}

function LaneSegment({
  value,
  long,
  short,
  weight,
  tone,
}: {
  value: string;
  long: string;
  short: string;
  weight: number;
  tone: "soft" | "softer" | "plain";
}) {
  return (
    <button
      type="button"
      {...stylex.props(
        styles.laneSegment,
        tone === "soft" && styles.laneSoft,
        tone === "softer" && styles.laneSofter,
      )}
      style={{ flexGrow: weight }}
    >
      <b {...stylex.props(styles.laneValue)}>{value}</b>
      <span {...stylex.props(styles.laneLongLabel)}>{long}</span>
      <span {...stylex.props(styles.laneShortLabel)}>{short}</span>
    </button>
  );
}

function LaneCard() {
  return (
    <article {...stylex.props(styles.laneCard)}>
      <div {...stylex.props(styles.laneRow)}>
        <span {...stylex.props(styles.laneTitle)}>
          Who can invoke <b {...stylex.props(styles.laneTotal)}>{SKILL_TOTAL}</b>
          <Info aria-hidden="true" size={12} {...stylex.props(styles.faint)} />
        </span>
        <div {...stylex.props(styles.laneSegments)}>
          <LaneSegment value="185" long="you or the model" short="both" weight={4} tone="soft" />
          <LaneSegment value="18" long="model only" short="model" weight={1.4} tone="softer" />
          <LaneSegment value="87" long="you only" short="you" weight={1.6} tone="plain" />
        </div>
      </div>
      <div {...stylex.props(styles.laneRow)}>
        <span {...stylex.props(styles.laneTitle)}>
          Prompt cost <b {...stylex.props(styles.laneTotal)}>12.5k</b>
          <Info aria-hidden="true" size={12} {...stylex.props(styles.faint)} />
        </span>
        <div {...stylex.props(styles.laneSegments)}>
          <LaneSegment
            value="9.3k"
            long="· 214 skills used in 30 days"
            short="used"
            weight={2.4}
            tone="soft"
          />
          <LaneSegment
            value="3.2k"
            long="· 76 skills not used in 30 days"
            short="idle"
            weight={1.6}
            tone="plain"
          />
        </div>
      </div>
    </article>
  );
}

function SkillsScreen({
  rows,
  onOpenSkill,
  onFix,
}: {
  rows: readonly SkillRow[];
  onOpenSkill: (skill: SkillRow) => void;
  onFix: (skill: SkillRow) => void;
}) {
  const [query, setQuery] = useState("");
  const [scope, setScope] = useState<Scope>("All");
  const visibleSkills = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    return rows.filter(
      (skill) =>
        (!normalized ||
          skill.name.includes(normalized) ||
          skill.description.toLowerCase().includes(normalized)) &&
        (scope === "All" ||
          skill.locations.includes(scope === "Global" ? "Global" : "agent-studio")),
    );
  }, [rows, query, scope]);
  const filtered = query.trim() !== "" || scope !== "All";
  const attention = visibleSkills.filter((skill) => skill.state);
  const healthy = visibleSkills.filter((skill) => !skill.state);
  return (
    <>
      <PageHeader title="Skills" />
      <div {...stylex.props(styles.toolbar)}>
        <label {...stylex.props(styles.filterInput)}>
          <Search aria-hidden="true" size={13} />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            aria-label="Filter skills"
            placeholder="Filter skills…"
            {...stylex.props(styles.input)}
          />
        </label>
        <div
          role="group"
          aria-label="Skill scope"
          {...stylex.props(styles.segmented, styles.inlineHideBelowNarrow)}
        >
          {(["All", "Global", "Project"] satisfies Scope[]).map((item) => (
            <button
              key={item}
              type="button"
              aria-pressed={scope === item}
              onClick={() => setScope(item)}
              {...stylex.props(styles.segment, scope === item && styles.segmentOn)}
            >
              {item}
              {item === "Project" && <ChevronDown aria-hidden="true" size={12} />}
            </button>
          ))}
        </div>
        <button type="button" {...stylex.props(styles.outlineButton, styles.inlineHideBelowMedium)}>
          <ListFilter aria-hidden="true" size={13} />
          Filter
        </button>
        <span {...stylex.props(styles.toolbarEnd)}>
          <span {...stylex.props(styles.toolbarCount)}>
            {filtered ? visibleSkills.length : SKILL_TOTAL} skills
          </span>
          <button type="button" {...stylex.props(styles.outlineButton, styles.inlineHideBelowWide)}>
            <span {...stylex.props(styles.faint)}>Sort:</span> Name
            <ChevronDown aria-hidden="true" size={12} />
          </button>
          <span
            role="group"
            aria-label="Layout"
            {...stylex.props(styles.segmented, styles.inlineHideBelowWide)}
          >
            <button
              type="button"
              aria-label="List"
              aria-pressed="true"
              {...stylex.props(styles.segment, styles.segmentOn)}
            >
              <List aria-hidden="true" size={14} />
            </button>
            <button
              type="button"
              aria-label="Grid"
              aria-pressed="false"
              {...stylex.props(styles.segment)}
            >
              <LayoutGrid aria-hidden="true" size={14} />
            </button>
          </span>
        </span>
      </div>
      <div {...stylex.props(styles.view)}>
        <section {...stylex.props(styles.page)}>
          <div {...stylex.props(styles.groups)}>
            {attention.length > 0 && (
              <SkillGroup
                label="Needs attention"
                count={attention.length}
                rows={attention}
                onOpenSkill={onOpenSkill}
                onFix={onFix}
              />
            )}
            {healthy.length > 0 && (
              <SkillGroup
                label="Healthy"
                count={healthy.length}
                rows={healthy}
                onOpenSkill={onOpenSkill}
                onFix={onFix}
              />
            )}
            {visibleSkills.length === 0 && (
              <p {...stylex.props(styles.empty)}>No skills match this filter.</p>
            )}
          </div>
        </section>
      </div>
    </>
  );
}

function PluginsScreen({ onOpenSkill }: { onOpenSkill: (skill: SkillRow) => void }) {
  return (
    <>
      <PageHeader
        title="Plugin skills"
        subtitle="Shipped inside agent plugins and updated with them - managed by the plugin, not by Skill Studio."
      />
      <div {...stylex.props(styles.view)}>
        <section {...stylex.props(styles.page)}>
          {pluginGroups.map((group) => (
            <article key={group.name} {...stylex.props(styles.pluginGroup)}>
              <header {...stylex.props(styles.pluginHeader)}>
                <AgentIcon agent={group.agent} size={14} />
                <strong {...stylex.props(styles.pluginName)}>{group.name}</strong>
                <small {...stylex.props(styles.faint)}>v{group.version}</small>
                <small {...stylex.props(styles.pluginCount)}>{group.skills.length} skills</small>
              </header>
              <div {...stylex.props(styles.pluginCard)}>
                {group.skills.map(([name, description, tokenCount]) => (
                  <button
                    key={name}
                    type="button"
                    onClick={() =>
                      onOpenSkill({
                        ...skillNamed(name),
                        description,
                        source: `${group.name} plugin`,
                        harnesses: [group.agent],
                        claudeLink: undefined,
                        fullTokens: tokenCount,
                      })
                    }
                    {...stylex.props(styles.pluginRow)}
                  >
                    <span {...stylex.props(styles.pluginSkill, styles.truncate)}>{name}</span>
                    <span {...stylex.props(styles.pluginDescription, styles.hideBelowNarrow)}>
                      {description}
                    </span>
                    <span {...stylex.props(styles.number)}>{tokenCount}</span>
                  </button>
                ))}
              </div>
            </article>
          ))}
        </section>
      </div>
    </>
  );
}

const HEAT_WEEKS = 48;
const heat = Array.from({ length: HEAT_WEEKS * 7 }, (_, index) => {
  const day = index % 7;
  const activity = (index * 37 + Math.floor(index / 7) * 13) % 101;
  if (day >= 5) return activity < 70 ? 1 : 0;
  if (activity < 18) return 0;
  return 1 + (activity % 4);
});
const heatMonths = [
  "Dec",
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
];

const activityRows = [
  ["sdk-migration", "15h ago", 70],
  ["commit", "just now", 47],
  ["agent-browser", "13h ago", 35],
  ["release-notes", "16h ago", 20],
  ["code-review", "3d ago", 14],
  ["design-vocabulary", "1d ago", 14],
  ["sentry-cli", "12h ago", 11],
] satisfies ReadonlyArray<readonly [string, string, number]>;

const activityBars = [
  {
    label: "Agent",
    rows: [
      { name: "Claude Code", agent: "claude", value: 297 },
      { name: "Cursor", agent: "cursor", value: 10 },
      { name: "Grok Build", agent: "grok", value: 3 },
    ],
  },
  {
    label: "Trigger",
    rows: [
      { name: "Typed", value: 91 },
      { name: "Called by the model", value: 206 },
      { name: "SKILL.md read", value: 13 },
    ],
  },
] satisfies ReadonlyArray<{
  label: string;
  rows: ReadonlyArray<{ name: string; agent?: AgentId; value: number }>;
}>;

function ActivityScreen({ onOpenSkill }: { onOpenSkill: (skill: SkillRow) => void }) {
  const [timeWindow, setTimeWindow] = useState<ActivityWindow>("30d");
  return (
    <>
      <PageHeader title="Activity" subtitle="From every agent turned on in Settings" />
      <div {...stylex.props(styles.toolbar)}>
        <button type="button" {...stylex.props(styles.outlineButton)}>
          <Layers aria-hidden="true" size={13} />
          All agents
          <ChevronDown aria-hidden="true" size={12} />
        </button>
        <button type="button" {...stylex.props(styles.outlineButton)}>
          Any trigger
          <ChevronDown aria-hidden="true" size={12} />
        </button>
      </div>
      <div {...stylex.props(styles.view)}>
        <section {...stylex.props(styles.page, styles.activityLayout)}>
          <div {...stylex.props(styles.activityMain)}>
            <div>
              <div {...stylex.props(styles.sectionTop)}>
                <span {...stylex.props(styles.caption)}>Last 48 weeks</span>
                <span {...stylex.props(styles.sectionMeta)}>6,867 uses</span>
              </div>
              <div {...stylex.props(styles.heatFrame)}>
                <span />
                <div {...stylex.props(styles.heatMonths)} aria-hidden="true">
                  {heatMonths.map((month) => (
                    <span key={month}>{month}</span>
                  ))}
                </div>
                <div {...stylex.props(styles.weekdays)} aria-hidden="true">
                  <span>Mon</span>
                  <span />
                  <span>Wed</span>
                  <span />
                  <span>Fri</span>
                  <span />
                  <span />
                </div>
                <div
                  aria-label="Skill uses for the last 48 weeks"
                  {...stylex.props(styles.heatmap)}
                >
                  {heat.map((level, index) => (
                    <i
                      key={index}
                      {...stylex.props(
                        styles.heatCell,
                        level === 1 && styles.heat1,
                        level === 2 && styles.heat2,
                        level === 3 && styles.heat3,
                        level === 4 && styles.heat4,
                      )}
                    />
                  ))}
                </div>
              </div>
              <div {...stylex.props(styles.heatLegend)} aria-hidden="true">
                Less
                <i {...stylex.props(styles.legendCell, styles.heatCellEmpty)} />
                <i {...stylex.props(styles.legendCell, styles.heat1)} />
                <i {...stylex.props(styles.legendCell, styles.heat2)} />
                <i {...stylex.props(styles.legendCell, styles.heat3)} />
                <i {...stylex.props(styles.legendCell, styles.heat4)} />
                More
              </div>
            </div>
            <div>
              <div {...stylex.props(styles.sectionTop)}>
                <span {...stylex.props(styles.caption)}>By skill</span>
                <div role="group" aria-label="Time window" {...stylex.props(styles.segmented)}>
                  {(["24h", "7d", "14d", "30d"] satisfies ActivityWindow[]).map((item) => (
                    <button
                      key={item}
                      type="button"
                      aria-pressed={timeWindow === item}
                      onClick={() => setTimeWindow(item)}
                      {...stylex.props(
                        styles.segment,
                        styles.segmentSmall,
                        timeWindow === item && styles.segmentOn,
                      )}
                    >
                      {item}
                    </button>
                  ))}
                </div>
              </div>
              {activityRows.map(([name, lastUsed, uses]) => (
                <button
                  key={name}
                  type="button"
                  onClick={() => onOpenSkill(skillNamed(name))}
                  {...stylex.props(styles.activityRow)}
                >
                  <span {...stylex.props(styles.truncate)}>{name}</span>
                  <span {...stylex.props(styles.faint)}>{lastUsed}</span>
                  <span {...stylex.props(styles.number)}>{uses}</span>
                </button>
              ))}
              <button type="button" {...stylex.props(styles.showMore)}>
                Show 16 more
              </button>
            </div>
          </div>
          <aside {...stylex.props(styles.activitySide)}>
            <div>
              <strong {...stylex.props(styles.sideTitle)}>Last 30 days</strong>
              <span {...stylex.props(styles.sideMeta)}>310 uses</span>
            </div>
            {activityBars.map((group) => (
              <div key={group.label} {...stylex.props(styles.barGroup)}>
                <span {...stylex.props(styles.sideLabel)}>{group.label}</span>
                {group.rows.map((row) => (
                  <div key={row.name} {...stylex.props(styles.barRow)}>
                    <span {...stylex.props(styles.barName)}>
                      {"agent" in row && <AgentIcon agent={row.agent} size={13} />}
                      {row.name}
                      <span {...stylex.props(styles.barValue)}>{row.value}</span>
                    </span>
                    <span {...stylex.props(styles.barTrack)}>
                      <i
                        {...stylex.props(styles.barFill)}
                        style={{ width: `${Math.max(1, (row.value / 310) * 100)}%` }}
                      />
                    </span>
                  </div>
                ))}
              </div>
            ))}
            <div {...stylex.props(styles.barGroup)}>
              <span {...stylex.props(styles.sideLabel)}>Busiest day</span>
              <span {...stylex.props(styles.barName)}>
                Fri, Oct 23
                <span {...stylex.props(styles.barValue)}>20 uses</span>
              </span>
            </div>
            <p {...stylex.props(styles.sideHint)}>Click a day to see its details here.</p>
          </aside>
        </section>
      </div>
    </>
  );
}

function ParkedScreen({
  parked,
  onOpenSkill,
  onUnpark,
}: {
  parked: readonly SkillRow[];
  onOpenSkill: (skill: SkillRow) => void;
  onUnpark: (skill: SkillRow) => void;
}) {
  return (
    <>
      <PageHeader title="Parked" subtitle="Turned off for every agent. Unpark to bring one back." />
      <div {...stylex.props(styles.view)}>
        <section {...stylex.props(styles.page)}>
          <div {...stylex.props(styles.groups)}>
            {parked.map((skill) => (
              <SkillListRow
                key={skill.name}
                skill={skill}
                onOpen={() => onOpenSkill(skill)}
                onFix={() => onUnpark(skill)}
              />
            ))}
            {parked.length === 0 && <p {...stylex.props(styles.empty)}>Nothing is parked.</p>}
          </div>
        </section>
      </div>
    </>
  );
}

const destinations = [
  { id: "universal", name: "Universal", global: "~/.agents/skills", project: ".agents/skills" },
  { id: "claude", name: "Claude Code", global: "~/.claude/skills", project: ".claude/skills" },
  { id: "codex", name: "Codex", global: "~/.codex/skills", project: ".codex/skills" },
  {
    id: "opencode",
    name: "OpenCode",
    global: "~/.config/opencode/skills",
    project: ".opencode/skills",
  },
  { id: "pi", name: "pi", global: "~/.pi/agent/skills", project: ".pi/skills" },
  { id: "cursor", name: "Cursor", global: "~/.cursor/skills", project: ".cursor/skills" },
  { id: "grok", name: "Grok Build", global: "~/.grok/skills", project: ".grok/skills" },
] satisfies ReadonlyArray<{ id: Harness; name: string; global: string; project: string }>;

const READS_UNIVERSAL: readonly Harness[] = ["codex", "opencode", "pi", "cursor", "grok"];

const methodNotes = {
  "skills.sh": "Tracked in .skill-lock.json. Installs to Universal.",
  dotagents: "Tracked in agents.toml. Installs to Universal.",
  Copy: "Copies the folder. No lock file tracks it.",
} satisfies Record<InstallMethod, string>;

function AddSkillSheet({
  onClose,
  onInstall,
}: {
  onClose: () => void;
  onInstall: (skill: SkillRow) => void;
}) {
  const { dialog, initialFocus: sourceInput, onKeyDown } = useModalFocus<HTMLInputElement>(onClose);
  const [source, setSource] = useState("");
  const [method, setMethod] = useState<InstallMethod>("skills.sh");
  const [scope, setScope] = useState<"Global" | "Project">("Global");
  const [picked, setPicked] = useState<readonly Harness[]>(["universal", "claude"]);
  const universal = picked.includes("universal");
  const coveredByUniversal = (id: Harness) => universal && READS_UNIVERSAL.includes(id);
  const togglePick = (id: Harness) =>
    setPicked((current) =>
      current.includes(id) ? current.filter((item) => item !== id) : [...current, id],
    );
  const trimmed = source.trim();
  const skillName = trimmed.replace(/\/$/, "").split("/").pop() ?? "";
  const harnesses = destinations
    .map((destination) => destination.id)
    .filter((id) => picked.includes(id) || coveredByUniversal(id));

  const installSkill = () =>
    onInstall({
      name: skillName || "new-skill",
      description: skillName.endsWith("frontend-design")
        ? "Create distinctive, production-grade frontend interfaces."
        : `Added from ${trimmed}.`,
      source: trimmed.split("/").slice(0, 2).join("/"),
      locations: [scope === "Global" ? "Global" : "agent-studio"],
      harnesses,
      claudeLink: picked.includes("claude") && universal ? "linked" : undefined,
      method,
      promptTokens: 12,
      fullTokens: "1.4k",
    });

  return (
    <div {...stylex.props(styles.sheetBackdrop)}>
      <button
        type="button"
        aria-label="Close add skill"
        tabIndex={-1}
        onClick={onClose}
        {...stylex.props(styles.sheetScrim)}
      />
      <section
        ref={dialog}
        role="dialog"
        aria-modal="true"
        aria-label="Add skill"
        onKeyDown={onKeyDown}
        {...stylex.props(styles.sheet)}
      >
        <header {...stylex.props(styles.sheetHeader)}>
          <h2 {...stylex.props(styles.sheetTitle)}>Add skill</h2>
          <button
            type="button"
            aria-label="Close"
            onClick={onClose}
            {...stylex.props(styles.iconButton)}
          >
            <X aria-hidden="true" size={15} />
          </button>
        </header>
        <div role="tablist" aria-label="Add skill method" {...stylex.props(styles.sheetTabs)}>
          <button
            type="button"
            role="tab"
            aria-selected="true"
            {...stylex.props(styles.sheetTab, styles.sheetTabOn)}
          >
            Add by source
          </button>
          <button type="button" role="tab" aria-selected="false" {...stylex.props(styles.sheetTab)}>
            Browse skills.sh
          </button>
        </div>
        <div {...stylex.props(styles.sheetBody)}>
          <label {...stylex.props(styles.field)}>
            <span {...stylex.props(styles.caption)}>Source</span>
            <input
              ref={sourceInput}
              value={source}
              onChange={(event) => setSource(event.target.value)}
              aria-label="Skill source"
              placeholder="owner/repo, a GitHub URL, a skills.sh URL, or a local path"
              {...stylex.props(styles.textInput)}
            />
            <small {...stylex.props(styles.fieldHelp)}>
              {trimmed ? `github · ${trimmed}` : "Paste a repo, URL, or path to get started."}
            </small>
          </label>
          {skillName && (
            <div {...stylex.props(styles.field)}>
              <span {...stylex.props(styles.caption)}>Skills</span>
              <span {...stylex.props(styles.foundSkill)}>
                <Folder aria-hidden="true" size={14} {...stylex.props(styles.faint)} />
                {skillName}
              </span>
            </div>
          )}
          <div {...stylex.props(styles.field)}>
            <span {...stylex.props(styles.caption)}>Method</span>
            <div role="group" aria-label="Install method" {...stylex.props(styles.segmented)}>
              {(["skills.sh", "dotagents", "Copy"] satisfies InstallMethod[]).map((item) => (
                <button
                  key={item}
                  type="button"
                  aria-pressed={method === item}
                  onClick={() => setMethod(item)}
                  {...stylex.props(styles.segment, method === item && styles.segmentOn)}
                >
                  {item}
                </button>
              ))}
            </div>
            <small {...stylex.props(styles.fieldHelp)}>{methodNotes[method]}</small>
          </div>
          <div {...stylex.props(styles.field)}>
            <span {...stylex.props(styles.caption)}>Scope</span>
            <div role="group" aria-label="Install scope" {...stylex.props(styles.segmented)}>
              {(["Global", "Project"] as const).map((item) => (
                <button
                  key={item}
                  type="button"
                  aria-pressed={scope === item}
                  onClick={() => setScope(item)}
                  {...stylex.props(styles.segment, scope === item && styles.segmentOn)}
                >
                  {item}
                </button>
              ))}
            </div>
          </div>
          <div {...stylex.props(styles.field)}>
            <span {...stylex.props(styles.caption)}>Destination</span>
            {destinations.map((destination) => {
              const covered = coveredByUniversal(destination.id);
              const checked = covered || picked.includes(destination.id);
              return (
                <label key={destination.id} {...stylex.props(styles.destination)}>
                  {destination.id === "universal" ? (
                    <Folder aria-hidden="true" size={15} />
                  ) : (
                    <AgentIcon agent={destination.id} size={15} />
                  )}
                  <span {...stylex.props(styles.destinationCopy)}>
                    <span>{destination.name}</span>
                    <code {...stylex.props(styles.destinationPath)}>
                      {scope === "Global" ? destination.global : destination.project}
                    </code>
                  </span>
                  <input
                    type="checkbox"
                    checked={checked}
                    disabled={covered}
                    onChange={() => togglePick(destination.id)}
                    {...stylex.props(styles.checkboxInput)}
                  />
                  <span
                    aria-hidden="true"
                    {...stylex.props(
                      styles.checkbox,
                      checked && styles.checkboxOn,
                      covered && styles.checkboxCovered,
                    )}
                  >
                    {checked && <Check size={11} strokeWidth={3} />}
                  </span>
                </label>
              );
            })}
          </div>
        </div>
        <footer {...stylex.props(styles.sheetFooter)}>
          <button type="button" onClick={onClose} {...stylex.props(styles.outlineButton)}>
            Cancel
          </button>
          <button
            type="button"
            disabled={!trimmed || harnesses.length === 0}
            onClick={installSkill}
            {...stylex.props(styles.solidButton)}
          >
            Add skill
          </button>
        </footer>
      </section>
    </div>
  );
}

function useModalFocus<T extends HTMLElement>(onClose: () => void) {
  const dialog = useRef<HTMLElement>(null);
  const initialFocus = useRef<T>(null);

  useEffect(() => {
    const previousFocus =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    initialFocus.current?.focus();
    return () => previousFocus?.focus();
  }, []);

  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    if (event.key !== "Tab") return;
    const focusable = dialog.current?.querySelectorAll<HTMLElement>(
      "button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex='-1'])",
    );
    if (!focusable?.length) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first?.focus();
    }
  };

  return { dialog, initialFocus, onKeyDown };
}

function CompareDialog({ skillName, onClose }: { skillName: string; onClose: () => void }) {
  const {
    dialog,
    initialFocus: closeButton,
    onKeyDown,
  } = useModalFocus<HTMLButtonElement>(onClose);

  return (
    <div {...stylex.props(styles.compareBackdrop)}>
      <section
        ref={dialog}
        role="dialog"
        aria-modal="true"
        aria-label={`Compare copies of ${skillName}`}
        onKeyDown={onKeyDown}
        {...stylex.props(styles.compareDialog)}
      >
        <header {...stylex.props(styles.compareHeader)}>
          <h2 {...stylex.props(styles.sheetTitle)}>Compare copies of {skillName}</h2>
          <button
            ref={closeButton}
            type="button"
            aria-label="Close comparison"
            onClick={onClose}
            {...stylex.props(styles.iconButton)}
          >
            <X aria-hidden="true" size={15} />
          </button>
        </header>
        <div {...stylex.props(styles.comparePickers)}>
          <button type="button" {...stylex.props(styles.outlineButton, styles.comparePicker)}>
            Global · ~/.agents/skills
            <ChevronDown aria-hidden="true" size={12} />
          </button>
          <button type="button" {...stylex.props(styles.outlineButton, styles.comparePicker)}>
            agent-studio · .agents/skills
            <ChevronDown aria-hidden="true" size={12} />
          </button>
        </div>
        <div {...stylex.props(styles.diff)}>
          <div {...stylex.props(styles.diffHeader)}>
            <span>SKILL.md</span>
            <span {...stylex.props(styles.danger)}>−2</span>
            <span {...stylex.props(styles.success)}>+2</span>
          </div>
          {[
            ["1", "1", "---", "context"],
            ["2", "2", `name: ${skillName}`, "context"],
            ["3", "3", "description: Create a verified commit.", "context"],
            ["4", "4", "---", "context"],
            ["5", "5", "", "context"],
            ["6", "6", "## Before committing", "context"],
            ["7", "", "- Run npm test.", "removed"],
            ["", "7", "+ Run pnpm test.", "added"],
            ["8", "", "- Commit to the current branch.", "removed"],
            ["", "8", "+ Create a branch before committing.", "added"],
          ].map(([before, after, content, kind], index) => (
            <div
              key={index}
              {...stylex.props(
                styles.diffRow,
                kind === "removed" && styles.diffRemoved,
                kind === "added" && styles.diffAdded,
              )}
            >
              <span>{before}</span>
              <span>{after}</span>
              <code>{content}</code>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}

function Toggle({ on, label, onToggle }: { on: boolean; label: string; onToggle: () => void }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      onClick={onToggle}
      {...stylex.props(styles.switchTrack, on && styles.switchTrackOn)}
    >
      <i {...stylex.props(styles.switchKnob, on && styles.switchKnobOn)} />
    </button>
  );
}

function DetailScreen({
  detail,
  onBack,
  onUpdate,
}: {
  detail: DetailState;
  onBack: () => void;
  onUpdate: () => void;
}) {
  const { skill } = detail;
  const [invocation, setInvocation] = useState<InvocationPolicy>("Both");
  const [expanded, setExpanded] = useState(false);
  const [offRows, setOffRows] = useState<readonly string[]>([]);
  const toggleRow = (row: string) =>
    setOffRows((current) =>
      current.includes(row) ? current.filter((item) => item !== row) : [...current, row],
    );
  const location = skill.locations[0] ?? "Global";
  const folderLabel = location === "Global" ? "Global folder" : "Project folder";
  const agents = skill.harnesses.filter((harness) => harness !== "universal");
  const lifecycle = skill.source.endsWith("plugin")
    ? "Plugin"
    : skill.method === "dotagents"
      ? "dotagents"
      : skill.method === "Copy"
        ? "Manual"
        : "skills.sh";
  const locationRows = [
    ...skill.locations.map((item) => ({ id: item, kind: "folder" as const })),
    ...(skill.harnesses.includes("claude") && skill.harnesses.includes("universal")
      ? [{ id: "Claude Code", kind: "claude" as const }]
      : []),
  ];
  return (
    <>
      <PageHeader
        parent={productMockDetailBackLabel(detail.from)}
        onParent={onBack}
        title={skill.name}
        actions={
          <>
            {skill.state === "update" && (
              <button type="button" onClick={onUpdate} {...stylex.props(styles.solidButton)}>
                Update
              </button>
            )}
            <button
              type="button"
              {...stylex.props(styles.outlineButton, styles.inlineHideBelowNarrow)}
            >
              <GitFork aria-hidden="true" size={13} />
              Fork
            </button>
            <button
              type="button"
              aria-label="More actions"
              {...stylex.props(styles.outlineButton, styles.squareButton)}
            >
              <MoreHorizontal aria-hidden="true" size={14} />
            </button>
          </>
        }
      />
      <div {...stylex.props(styles.view)}>
        <section {...stylex.props(styles.page, styles.detailLayout)}>
          <div {...stylex.props(styles.detailMain)}>
            <div {...stylex.props(styles.detailIntro)}>
              <h3 {...stylex.props(styles.detailTitle)}>{skill.name}</h3>
              <span {...stylex.props(styles.detailSource)}>{skill.source}</span>
              <p {...stylex.props(styles.detailDescription)}>{skill.description}</p>
              {skill.state === "update" && (
                <span {...stylex.props(styles.pill)}>Update available</span>
              )}
              {skill.state === "broken" && (
                <span {...stylex.props(styles.pill, styles.pillDanger)}>{skill.note}</span>
              )}
              {skill.state === "warning" && (
                <span {...stylex.props(styles.pill, styles.pillWarning)}>{skill.note}</span>
              )}
            </div>

            <article {...stylex.props(styles.card)}>
              <h4 {...stylex.props(styles.cardTitle)}>Locations</h4>
              {locationRows.map((row, index) => (
                <div key={row.id}>
                  <div {...stylex.props(styles.locationRow)}>
                    {row.kind === "folder" ? (
                      <button
                        type="button"
                        aria-expanded={index === 0 ? expanded : undefined}
                        onClick={() => index === 0 && setExpanded(!expanded)}
                        {...stylex.props(styles.locationMain)}
                      >
                        <ChevronRight
                          aria-hidden="true"
                          size={14}
                          {...stylex.props(
                            styles.locationChevron,
                            index === 0 && expanded && styles.locationChevronOpen,
                          )}
                        />
                        <Folder aria-hidden="true" size={15} {...stylex.props(styles.muted)} />
                        <span>{row.id}</span>
                      </button>
                    ) : (
                      <span {...stylex.props(styles.locationMain, styles.locationIndent)}>
                        <AgentIcon agent="claude" size={15} />
                        <span>Claude Code</span>
                        <Link2 aria-label="Linked" size={12} {...stylex.props(styles.faint)} />
                      </span>
                    )}
                    {row.kind === "folder" && (
                      <span {...stylex.props(styles.hideBelowNarrow)}>
                        <HarnessDiscs
                          harnesses={agents.filter((agent) => READS_UNIVERSAL.includes(agent))}
                        />
                      </span>
                    )}
                    <Toggle
                      on={!offRows.includes(row.id)}
                      label={`Turn on ${skill.name} in ${row.id}`}
                      onToggle={() => toggleRow(row.id)}
                    />
                    <button
                      type="button"
                      aria-label={`${row.id} actions`}
                      {...stylex.props(styles.iconButton)}
                    >
                      <MoreHorizontal aria-hidden="true" size={14} />
                    </button>
                  </div>
                  {index === 0 && expanded && (
                    <code {...stylex.props(styles.locationPath)}>
                      {row.id === "Global" ? "~/" : "agent-studio/"}.agents/skills/{skill.name}
                    </code>
                  )}
                </div>
              ))}
              <div {...stylex.props(styles.invocation)}>
                <span {...stylex.props(styles.caption)}>Invocation</span>
                <div {...stylex.props(styles.invocationRow)}>
                  <span {...stylex.props(styles.invocationName)}>
                    <Folder aria-hidden="true" size={15} {...stylex.props(styles.muted)} />
                    {folderLabel}
                  </span>
                  <div
                    role="group"
                    aria-label="Invocation policy"
                    {...stylex.props(styles.segmented)}
                  >
                    {(["Both", "User only", "Model only"] satisfies InvocationPolicy[]).map(
                      (label) => (
                        <button
                          key={label}
                          type="button"
                          aria-pressed={invocation === label}
                          onClick={() => setInvocation(label)}
                          {...stylex.props(
                            styles.segment,
                            invocation === label && styles.segmentOn,
                          )}
                        >
                          {label}
                        </button>
                      ),
                    )}
                  </div>
                </div>
                <p {...stylex.props(styles.invocationNote)}>
                  {invocation === "Both" &&
                    `Both: you can call /${skill.name} and the model can pick it.`}
                  {invocation === "User only" && `User only: only /${skill.name} starts it.`}
                  {invocation === "Model only" &&
                    `Model only: the model picks it; there is no /${skill.name} command.`}
                </p>
              </div>
            </article>

            <article {...stylex.props(styles.card)}>
              <div {...stylex.props(styles.cardTop)}>
                <h4 {...stylex.props(styles.cardTitle)}>SKILL.md</h4>
                <span {...stylex.props(styles.cardTopEnd)}>
                  <small {...stylex.props(styles.faint)}>{location} · Universal folder</small>
                  <button type="button" {...stylex.props(styles.outlineButton)}>
                    Edit
                  </button>
                </span>
              </div>
              <div {...stylex.props(styles.markdown)}>
                <h5 {...stylex.props(styles.markdownTitle)}>{skill.name}</h5>
                <p {...stylex.props(styles.markdownParagraph)}>{skill.description}</p>
                <h6 {...stylex.props(styles.markdownHeading)}>When to use this skill</h6>
                <p {...stylex.props(styles.markdownParagraph)}>
                  Use it when the work needs a repeatable workflow, clear constraints, and a result
                  that the project can verify.
                </p>
              </div>
            </article>
          </div>

          <dl {...stylex.props(styles.rail)}>
            <dt>Location</dt>
            <dd>{skill.locations.join(", ")}</dd>
            <dt>Agents</dt>
            <dd {...stylex.props(styles.railAgents)}>
              <HarnessDiscs harnesses={skill.harnesses} claudeLink={skill.claudeLink} />
              <span {...stylex.props(styles.faint)}>{agents.length}</span>
            </dd>
            <dt>Source</dt>
            <dd>
              <code {...stylex.props(styles.railMono)}>{skill.source}</code>
              <small {...stylex.props(styles.railSub)}>{lifecycle}</small>
            </dd>
            <dt>Lifecycle</dt>
            <dd>{lifecycle === "Manual" ? "Manual · Not tracked" : `${lifecycle} · Managed`}</dd>
            <dt>Tokens</dt>
            <dd>
              Prompt {skill.promptTokens} · Full {skill.fullTokens}
            </dd>
            <dt>Installed</dt>
            <dd>
              Sep 2, 2026 <span {...stylex.props(styles.faint)}>· Modified Sep 3, 2026</span>
            </dd>
          </dl>
        </section>
      </div>
    </>
  );
}

const UPDATABLE_NAMES = skills
  .filter((skill) => skill.state === "update")
  .map((skill) => skill.name);

const markUpdated = (skill: SkillRow): SkillRow => ({
  ...skill,
  state: undefined,
  note: undefined,
  fix: undefined,
});

export function ProductMock({ theme, onToggleTheme }: ProductMockProps) {
  const [activeView, setActiveView] = useState<ProductMockRootView>("home");
  const [detail, setDetail] = useState<DetailState | null>(null);
  const [installOpen, setInstallOpen] = useState(false);
  const [compareSkill, setCompareSkill] = useState<string | null>(null);
  const [parked, setParked] = useState<readonly SkillRow[]>(initialParked);
  const [updatedNames, setUpdatedNames] = useState<readonly string[]>([]);
  const rows = skills.map((skill) =>
    updatedNames.includes(skill.name) ? markUpdated(skill) : skill,
  );
  const [updateTotal, setUpdateTotal] = useState(UPDATE_TOTAL);
  const updateSkills = (names: readonly string[]) => {
    const fresh = names.filter((name) => !updatedNames.includes(name));
    setUpdatedNames([...updatedNames, ...fresh]);
    setUpdateTotal((total) => total - fresh.length);
    setDetail((current) =>
      current && names.includes(current.skill.name)
        ? { ...current, skill: markUpdated(current.skill) }
        : current,
    );
  };
  const navigate = (view: ProductMockRootView) => {
    setActiveView(view);
    setDetail(null);
    setInstallOpen(false);
    setCompareSkill(null);
  };
  const openSkill = (skill: SkillRow) => {
    setDetail({ skill, from: activeView });
    setInstallOpen(false);
    setCompareSkill(null);
  };
  const fixSkill = (skill: SkillRow) => {
    if (skill.fix === "Compare") setCompareSkill(skill.name);
    else if (skill.fix === "Update") updateSkills([skill.name]);
    else openSkill(skill);
  };
  return (
    <div
      role="region"
      aria-label="Interactive Skill Studio product preview"
      {...stylex.props(styles.window, theme === "light" && productDemoLightTheme)}
    >
      <Sidebar
        activeView={activeView}
        parkedCount={parked.length}
        onNavigate={navigate}
        onAddSkill={() => {
          setInstallOpen(true);
          setCompareSkill(null);
        }}
        theme={theme}
        onToggleTheme={onToggleTheme}
      />
      <div {...stylex.props(styles.contentColumn)}>
        <div {...stylex.props(styles.dragStrip, styles.contentStrip)} />
        <main
          key={detail ? `detail-${detail.skill.name}` : activeView}
          {...stylex.props(styles.panel)}
        >
          {detail ? (
            <DetailScreen
              detail={detail}
              onBack={() => setDetail(null)}
              onUpdate={() => updateSkills([detail.skill.name])}
            />
          ) : activeView === "home" ? (
            <HomeScreen
              rows={rows}
              updateTotal={updateTotal}
              onOpenSkill={openSkill}
              onFix={fixSkill}
              onUpdateAll={() => {
                updateSkills(UPDATABLE_NAMES);
                setUpdateTotal(0);
              }}
            />
          ) : activeView === "skills" ? (
            <SkillsScreen rows={rows} onOpenSkill={openSkill} onFix={fixSkill} />
          ) : activeView === "plugins" ? (
            <PluginsScreen onOpenSkill={openSkill} />
          ) : activeView === "activity" ? (
            <ActivityScreen onOpenSkill={openSkill} />
          ) : (
            <ParkedScreen
              parked={parked}
              onOpenSkill={openSkill}
              onUnpark={(skill) =>
                setParked((current) => current.filter((item) => item.name !== skill.name))
              }
            />
          )}
        </main>
      </div>
      {installOpen && (
        <AddSkillSheet
          onClose={() => setInstallOpen(false)}
          onInstall={(skill) => {
            setActiveView("skills");
            setDetail({ skill, from: "skills" });
            setInstallOpen(false);
          }}
        />
      )}
      {compareSkill && (
        <CompareDialog skillName={compareSkill} onClose={() => setCompareSkill(null)} />
      )}
    </div>
  );
}

const MOBILE = "@media (max-width: 680px)";
// The panel width follows the hero column, not the viewport, so rows, toolbars and the lane
// card use container queries against the panel.
const PANEL_BELOW_WIDE = "@container (max-width: 759px)";
const PANEL_BELOW_MEDIUM = "@container (max-width: 619px)";
const PANEL_BELOW_NARROW = "@container (max-width: 459px)";
const PANEL_MEDIUM_ONLY = "@container (min-width: 460px) and (max-width: 619px)";
const LANE_LONG_LABELS = "@container (min-width: 380px)";

const fontStack = "-apple-system, BlinkMacSystemFont, 'SF Pro Text', 'Segoe UI', sans-serif";
const monoStack = "ui-monospace, SFMono-Regular, Menlo, monospace";

const styles = stylex.create({
  window: {
    backgroundColor: tokens.surface,
    borderColor: tokens.border,
    borderRadius: 10,
    borderStyle: "solid",
    borderWidth: 1,
    boxShadow: "0 2px 5px oklch(0 0 0 / .22), 0 20px 55px oklch(0 0 0 / .34)",
    color: tokens.text,
    display: "flex",
    fontFamily: fontStack,
    fontSize: 13,
    height: 600,
    isolation: "isolate",
    overflow: "hidden",
    position: "relative",
    WebkitFontSmoothing: "antialiased",
    width: "100%",
    [MOBILE]: { borderRadius: "min(3vw, 10px)", flexDirection: "column", height: 540 },
  },

  // Shell
  sidebar: {
    display: "flex",
    flexDirection: "column",
    flexShrink: 0,
    width: 220,
    [MOBILE]: {
      alignItems: "center",
      flexDirection: "row",
      gap: 4,
      paddingInline: 6,
      width: "100%",
    },
  },
  dragStrip: { alignItems: "center", display: "flex", flexShrink: 0, height: 36 },
  contentStrip: { [MOBILE]: { display: "none" } },
  traffic: { display: "flex", gap: 8, paddingInline: 14, [MOBILE]: { display: "none" } },
  trafficDot: {
    backgroundColor: tokens.strongBorder,
    borderRadius: "50%",
    display: "block",
    height: 12,
    width: 12,
  },
  sidebarTitleRow: {
    alignItems: "center",
    display: "flex",
    height: 40,
    justifyContent: "space-between",
    paddingInline: "14px 8px",
    [MOBILE]: { display: "none" },
  },
  sidebarTitle: { fontSize: 12, fontWeight: 600 },
  sidebarTitleActions: { display: "flex", gap: 2 },
  iconButton: {
    alignItems: "center",
    backgroundColor: { default: "transparent", ":hover": tokens.hover },
    borderRadius: 4,
    borderWidth: 0,
    color: { default: tokens.faint, ":hover": tokens.text },
    cursor: "pointer",
    display: "inline-flex",
    flexShrink: 0,
    height: 24,
    justifyContent: "center",
    padding: 0,
    transition: "background-color 150ms ease-out, color 150ms ease-out",
    width: 24,
  },
  productNav: {
    display: "flex",
    flexDirection: "column",
    gap: 1,
    paddingInline: 8,
    [MOBILE]: { flex: 1, flexDirection: "row", gap: 2, minWidth: 0, paddingInline: 0 },
  },
  navGap: {
    display: "flex",
    flexDirection: "column",
    paddingTop: 12,
    [MOBILE]: { display: "contents" },
  },
  navItem: {
    alignItems: "center",
    backgroundColor: { default: "transparent", ":hover": tokens.hover },
    borderRadius: 4,
    borderWidth: 0,
    color: { default: tokens.muted, ":hover": tokens.text },
    cursor: "pointer",
    display: "grid",
    fontSize: 13,
    gap: 8,
    gridTemplateColumns: "14px 1fr auto",
    height: 26,
    paddingInline: 8,
    textAlign: "left",
    transition: "background-color 150ms ease-out, color 150ms ease-out",
    ":active": { transform: "scale(.97)" },
    [MOBILE]: {
      display: "flex",
      flex: 1,
      fontSize: 12,
      gap: 5,
      height: 40,
      justifyContent: "center",
      minWidth: 0,
      paddingInline: 4,
    },
  },
  navIcon: { flexShrink: 0 },
  navLabelCollapsible: { display: { default: "inline", [MOBILE]: "none" } },
  navItemActive: { backgroundColor: tokens.active, color: tokens.text },
  navCount: {
    color: tokens.faint,
    fontSize: 11,
    fontVariantNumeric: "tabular-nums",
    [MOBILE]: { display: "none" },
  },
  sidebarFooter: {
    alignItems: "center",
    display: "flex",
    justifyContent: "space-between",
    marginTop: "auto",
    padding: "8px 8px 10px",
    [MOBILE]: { marginTop: 0, padding: 0 },
  },
  syncButton: {
    alignItems: "center",
    backgroundColor: { default: "transparent", ":hover": tokens.hover },
    borderRadius: 4,
    borderWidth: 0,
    color: { default: tokens.muted, ":hover": tokens.text },
    cursor: "pointer",
    display: "flex",
    fontSize: 12,
    gap: 6,
    height: 24,
    paddingInline: 6,
    [MOBILE]: { display: "none" },
  },
  footerActions: { display: "flex", gap: 2 },
  footerOnlyDesktop: { [MOBILE]: { display: "none" } },
  themeButton: { [MOBILE]: { height: 32, width: 32 } },
  contentColumn: {
    display: "flex",
    flex: 1,
    flexDirection: "column",
    minHeight: 0,
    minWidth: 0,
    paddingBottom: 8,
    paddingRight: 8,
    [MOBILE]: { paddingInline: 6, paddingBottom: 6 },
  },
  panel: {
    animationDuration: "190ms",
    animationName: viewEnter,
    animationTimingFunction: "cubic-bezier(.16,1,.3,1)",
    backgroundColor: tokens.background,
    borderColor: tokens.border,
    borderRadius: 6,
    borderStyle: "solid",
    borderWidth: 1,
    containerType: "inline-size",
    display: "flex",
    flex: 1,
    flexDirection: "column",
    minHeight: 0,
    overflow: "hidden",
    "@media (prefers-reduced-motion: reduce)": { animationName: "none" },
  },

  // Page header and toolbar (PageShell)
  pageHeader: {
    alignItems: "center",
    borderBottomColor: tokens.subtleBorder,
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    display: "flex",
    flexShrink: 0,
    gap: 8,
    height: 40,
    minWidth: 0,
    paddingInline: 16,
    [MOBILE]: { paddingInline: 12 },
  },
  crumbs: {
    alignItems: "center",
    display: "flex",
    flexShrink: 0,
    fontSize: 12,
    fontWeight: 500,
    gap: 6,
    margin: 0,
    maxWidth: "60%",
    minWidth: 0,
  },
  crumbParent: {
    backgroundColor: "transparent",
    borderWidth: 0,
    color: { default: tokens.faint, ":hover": tokens.text },
    cursor: "pointer",
    fontSize: 12,
    padding: 0,
  },
  crumbChevron: { color: tokens.ghost, flexShrink: 0 },
  crumbTitle: { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  crumbSubtitle: {
    color: tokens.faint,
    fontSize: 12,
    minWidth: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  headerActions: { alignItems: "center", display: "flex", gap: 6, marginLeft: "auto" },
  toolbar: {
    alignItems: "center",
    borderBottomColor: tokens.subtleBorder,
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    display: "flex",
    flexShrink: 0,
    gap: 8,
    height: 44,
    paddingInline: 16,
    [MOBILE]: { paddingInline: 12 },
  },
  toolbarEnd: { alignItems: "center", display: "flex", gap: 8, marginLeft: "auto" },
  toolbarCount: { color: tokens.faint, fontSize: 12, whiteSpace: "nowrap" },
  filterInput: {
    alignItems: "center",
    borderColor: { default: tokens.border, ":focus-within": tokens.accent },
    borderRadius: 4,
    borderStyle: "solid",
    borderWidth: 1,
    color: tokens.faint,
    display: "flex",
    flex: "0 1 190px",
    gap: 6,
    height: 28,
    minWidth: 0,
    paddingInline: 8,
  },
  input: {
    backgroundColor: "transparent",
    borderWidth: 0,
    color: tokens.text,
    fontSize: 13,
    minWidth: 0,
    outline: "none",
    padding: 0,
    width: "100%",
    "::placeholder": { color: tokens.faint },
    [MOBILE]: { fontSize: 16 },
  },
  segmented: {
    alignSelf: "flex-start",
    borderColor: tokens.border,
    borderRadius: 4,
    borderStyle: "solid",
    borderWidth: 1,
    display: "inline-flex",
    flexShrink: 0,
    overflow: "hidden",
  },
  segment: {
    alignItems: "center",
    backgroundColor: { default: "transparent", ":hover": tokens.hover },
    borderColor: tokens.border,
    borderStyle: "solid",
    borderWidth: 0,
    borderLeftWidth: { default: 1, ":first-child": 0 },
    color: { default: tokens.muted, ":hover": tokens.text },
    cursor: "pointer",
    display: "inline-flex",
    fontSize: 12,
    gap: 4,
    height: 26,
    paddingInline: 10,
    whiteSpace: "nowrap",
  },
  segmentSmall: { fontSize: 11, height: 22, paddingInline: 8 },
  segmentOn: { backgroundColor: tokens.active, color: tokens.text, fontWeight: 500 },
  outlineButton: {
    alignItems: "center",
    backgroundColor: { default: "transparent", ":hover": tokens.hover },
    borderColor: tokens.border,
    borderRadius: 4,
    borderStyle: "solid",
    borderWidth: 1,
    color: tokens.text,
    cursor: "pointer",
    display: "inline-flex",
    flexShrink: 0,
    fontSize: 12,
    gap: 6,
    height: 28,
    paddingInline: 10,
    whiteSpace: "nowrap",
  },
  squareButton: { justifyContent: "center", paddingInline: 0, width: 28 },
  solidButton: {
    alignItems: "center",
    backgroundColor: { default: tokens.accentSolid, ":hover": tokens.accentSolidHover },
    borderRadius: 4,
    borderWidth: 0,
    color: "oklch(1 0 0)",
    cursor: { default: "pointer", ":disabled": "default" },
    display: "inline-flex",
    flexShrink: 0,
    fontSize: 12,
    fontWeight: 500,
    height: 28,
    opacity: { default: 1, ":disabled": 0.45 },
    paddingInline: 12,
    ":active": { transform: "scale(.97)" },
  },

  // Page body
  view: {
    flex: 1,
    minHeight: 0,
    overflow: "auto",
    scrollbarWidth: "none",
    "::-webkit-scrollbar": { display: "none" },
  },
  page: {
    display: "flex",
    flexDirection: "column",
    gap: 20,
    marginInline: "auto",
    maxWidth: 1200,
    padding: "20px 24px 28px",
    [MOBILE]: { gap: 14, padding: "14px 10px 20px" },
  },
  caption: {
    color: tokens.faint,
    fontSize: 11,
    letterSpacing: "0.06em",
    textTransform: "uppercase",
  },
  truncate: { minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  number: { color: tokens.muted, fontSize: 12, fontVariantNumeric: "tabular-nums" },
  empty: { color: tokens.faint, fontSize: 13, margin: 0, padding: 16, textAlign: "center" },
  danger: { color: tokens.danger },
  warning: { color: tokens.warning },
  accent: { color: tokens.accent },
  success: { color: "oklch(0.696 0.149 162.48)" },
  muted: { color: tokens.muted },
  faint: { color: tokens.faint },
  inlineHideBelowWide: { display: { default: "inline-flex", [PANEL_BELOW_WIDE]: "none" } },
  inlineHideBelowMedium: { display: { default: "inline-flex", [PANEL_BELOW_MEDIUM]: "none" } },
  inlineHideBelowNarrow: { display: { default: "inline-flex", [PANEL_BELOW_NARROW]: "none" } },
  hideBelowNarrow: { display: { default: null, [PANEL_BELOW_NARROW]: "none" } },

  // Home
  statGrid: {
    display: "grid",
    gap: 12,
    gridTemplateColumns: "repeat(3, minmax(0, 1fr))",
    [MOBILE]: { gap: 6 },
  },
  statWrap: { display: "flex", minWidth: 0, position: "relative" },
  statTile: {
    alignItems: "stretch",
    backgroundColor: { default: tokens.elevated, ":hover": tokens.surface },
    borderColor: tokens.subtleBorder,
    borderRadius: 6,
    borderStyle: "solid",
    borderWidth: 1,
    color: tokens.text,
    cursor: "pointer",
    display: "flex",
    flex: 1,
    flexDirection: "column",
    gap: 4,
    minWidth: 0,
    padding: "14px 16px",
    textAlign: "left",
    transition: "background-color 150ms ease-out, transform 150ms ease-out",
    ":active": { transform: "scale(.98)" },
    [MOBILE]: { padding: "10px 10px" },
  },
  statTileOn: { backgroundColor: tokens.accentSofter, borderColor: tokens.accent },
  statLabel: {
    color: tokens.faint,
    fontSize: 11,
    letterSpacing: "0.06em",
    textTransform: "uppercase",
  },
  statValue: {
    fontSize: 24,
    fontVariantNumeric: "tabular-nums",
    fontWeight: 600,
    letterSpacing: "-0.02em",
    lineHeight: 1.1,
    [MOBILE]: { fontSize: 20 },
  },
  statCaption: {
    color: tokens.faint,
    fontSize: 11,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  statCheck: { position: "absolute", right: 10, top: 10, [MOBILE]: { right: 4, top: 4 } },
  laneCard: {
    backgroundColor: tokens.elevated,
    borderColor: tokens.subtleBorder,
    borderRadius: 6,
    borderStyle: "solid",
    borderWidth: 1,
    display: "flex",
    flexDirection: "column",
    gap: 8,
    padding: "14px 16px",
    [MOBILE]: { padding: 10 },
  },
  laneRow: {
    alignItems: "center",
    display: "grid",
    gap: { default: 12, [PANEL_BELOW_MEDIUM]: 6 },
    gridTemplateColumns: { default: "170px minmax(0,1fr)", [PANEL_BELOW_MEDIUM]: "1fr" },
  },
  laneTitle: {
    alignItems: "center",
    color: tokens.muted,
    display: "flex",
    fontSize: 12,
    gap: 6,
    whiteSpace: "nowrap",
  },
  laneTotal: { color: tokens.text, fontWeight: 500 },
  laneSegments: { containerType: "inline-size", display: "flex", gap: 2, minWidth: 0 },
  laneSegment: {
    alignItems: "center",
    backgroundColor: tokens.raised,
    borderRadius: 4,
    borderWidth: 0,
    color: tokens.muted,
    cursor: "pointer",
    display: "flex",
    flexBasis: 0,
    fontSize: 12,
    gap: 4,
    height: 28,
    justifyContent: "center",
    minWidth: 0,
    overflow: "hidden",
    paddingInline: 8,
    whiteSpace: "nowrap",
    ":hover": { filter: "brightness(1.15)" },
  },
  laneSoft: { backgroundColor: tokens.accentSoft, color: tokens.text },
  laneSofter: { backgroundColor: tokens.accentSofter },
  laneValue: { fontWeight: 500 },
  laneLongLabel: {
    display: { default: "none", [LANE_LONG_LABELS]: "inline" },
    overflow: "hidden",
    textOverflow: "ellipsis",
  },
  laneShortLabel: { display: { default: "inline", [LANE_LONG_LABELS]: "none" } },

  // Skill list
  groups: { display: "flex", flexDirection: "column" },
  group: { display: "flex", flexDirection: "column" },
  groupHead: {
    alignItems: "center",
    backgroundColor: tokens.surface,
    borderRadius: 4,
    display: "flex",
    gap: 8,
    height: 28,
    paddingInline: 12,
    [MOBILE]: { paddingInline: 8 },
  },
  groupToggle: {
    alignItems: "center",
    backgroundColor: "transparent",
    borderWidth: 0,
    color: tokens.muted,
    cursor: "pointer",
    display: "flex",
    flex: 1,
    fontSize: 12,
    fontWeight: 500,
    gap: 8,
    height: "100%",
    minWidth: 0,
    padding: 0,
    textAlign: "left",
  },
  groupChevron: {
    color: tokens.faint,
    flexShrink: 0,
    transition: "transform 150ms ease-out",
    "@media (prefers-reduced-motion: reduce)": { transition: "none" },
  },
  groupChevronClosed: { transform: "rotate(-90deg)" },
  groupCount: { color: tokens.faint, fontVariantNumeric: "tabular-nums", fontWeight: 400 },
  groupAction: {
    backgroundColor: "transparent",
    borderWidth: 0,
    color: { default: tokens.accent, ":hover": tokens.accentHover },
    cursor: "pointer",
    fontSize: 12,
    padding: 0,
  },
  row: {
    alignItems: "center",
    backgroundColor: { default: "transparent", ":hover": tokens.hover },
    borderBottomColor: tokens.subtleBorder,
    borderBottomStyle: "solid",
    borderBottomWidth: { default: 1, ":last-child": 0 },
    columnGap: 12,
    display: "grid",
    gridTemplateColumns: {
      default: "14px minmax(0,1fr) 120px 84px minmax(0,1.1fr) 72px",
      [PANEL_MEDIUM_ONLY]: "14px minmax(0,1fr) 84px minmax(0,1fr) 64px",
      [PANEL_BELOW_NARROW]: "14px minmax(0,1fr) 64px",
    },
    height: 32,
    minWidth: 0,
    paddingInline: 12,
    position: "relative",
    [MOBILE]: { columnGap: 8, paddingInline: 8 },
  },
  rowSlot: { alignItems: "center", display: "flex" },
  rowName: {
    backgroundColor: "transparent",
    borderWidth: 0,
    color: tokens.text,
    cursor: "pointer",
    display: "flex",
    fontSize: 13,
    minWidth: 0,
    padding: 0,
    textAlign: "left",
    "::after": { content: '""', inset: 0, position: "absolute" },
  },
  locationCell: {
    alignItems: "center",
    color: tokens.muted,
    display: { default: "flex", [PANEL_BELOW_MEDIUM]: "none" },
    fontSize: 12,
    gap: 6,
    minWidth: 0,
  },
  locationChip: { alignItems: "center", display: "inline-flex", gap: 4, minWidth: 0 },
  discs: { alignItems: "center", display: "inline-flex" },
  disc: {
    alignItems: "center",
    backgroundColor: tokens.surface,
    borderRadius: "50%",
    boxShadow: `0 0 0 2px ${tokens.background}`,
    color: tokens.text,
    display: "inline-flex",
    flexShrink: 0,
    height: 20,
    justifyContent: "center",
    position: "relative",
    width: 20,
  },
  discOverlap: { marginLeft: -4 },
  discBadge: {
    alignItems: "center",
    backgroundColor: tokens.background,
    borderRadius: "50%",
    bottom: -3,
    display: "inline-flex",
    height: 10,
    justifyContent: "center",
    position: "absolute",
    right: -3,
    width: 10,
  },
  discOverflow: { color: tokens.faint, fontSize: 11, marginLeft: 4 },
  noteCell: {
    alignItems: "center",
    display: { default: "flex", [PANEL_BELOW_NARROW]: "none" },
    gap: 10,
    minWidth: 0,
  },
  noteText: {
    color: tokens.muted,
    flex: 1,
    fontSize: 12,
    minWidth: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  tokenPair: {
    display: "inline-flex",
    flexShrink: 0,
    fontSize: 12,
    fontVariantNumeric: "tabular-nums",
    gap: 6,
  },
  fixCell: { display: "flex", justifyContent: "flex-end", position: "relative", zIndex: 1 },
  fixButton: {
    backgroundColor: { default: "transparent", ":hover": tokens.raised },
    borderRadius: 4,
    borderWidth: 0,
    color: { default: tokens.muted, ":hover": tokens.text },
    cursor: "pointer",
    fontSize: 12,
    height: 24,
    paddingInline: 8,
    transition: "background-color 150ms ease-out, color 150ms ease-out",
    ":active": { transform: "scale(.96)" },
  },

  // Plugins
  pluginGroup: { display: "flex", flexDirection: "column", gap: 6 },
  pluginHeader: {
    alignItems: "center",
    display: "flex",
    fontSize: 11,
    gap: 8,
    paddingInline: 12,
  },
  pluginName: { fontSize: 13, fontWeight: 600 },
  pluginCount: { color: tokens.faint, marginLeft: "auto" },
  pluginCard: {
    backgroundColor: tokens.elevated,
    borderColor: tokens.subtleBorder,
    borderRadius: 6,
    borderStyle: "solid",
    borderWidth: 1,
    overflow: "hidden",
  },
  pluginRow: {
    alignItems: "center",
    backgroundColor: { default: "transparent", ":hover": tokens.hover },
    borderBottomColor: tokens.subtleBorder,
    borderBottomStyle: "solid",
    borderBottomWidth: { default: 1, ":last-child": 0 },
    borderWidth: 0,
    color: tokens.text,
    columnGap: 16,
    cursor: "pointer",
    display: "grid",
    gridTemplateColumns: {
      default: "minmax(0,0.9fr) minmax(0,1.6fr) 48px",
      [PANEL_BELOW_NARROW]: "minmax(0,1fr) 48px",
    },
    height: 42,
    paddingInline: 12,
    textAlign: "left",
    width: "100%",
  },
  pluginSkill: { fontSize: 13, fontWeight: 600 },
  pluginDescription: {
    color: tokens.muted,
    fontSize: 12,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },

  // Activity
  activityLayout: {
    alignItems: "start",
    display: "grid",
    gap: 24,
    gridTemplateColumns: { default: "minmax(0,1fr) 260px", [PANEL_BELOW_WIDE]: "minmax(0,1fr)" },
  },
  activityMain: { display: "flex", flexDirection: "column", gap: 24, minWidth: 0 },
  sectionTop: {
    alignItems: "center",
    display: "flex",
    justifyContent: "space-between",
    marginBottom: 10,
  },
  sectionMeta: { color: tokens.muted, fontSize: 12, fontVariantNumeric: "tabular-nums" },
  heatFrame: {
    color: tokens.faint,
    columnGap: 8,
    display: "grid",
    fontSize: 10,
    gridTemplateColumns: "24px minmax(0,1fr)",
    rowGap: 4,
  },
  heatMonths: {
    display: "grid",
    gridTemplateColumns: "repeat(12, minmax(0,1fr))",
    whiteSpace: "nowrap",
  },
  weekdays: { display: "grid", gridTemplateRows: "repeat(7, minmax(0,1fr))", rowGap: 3 },
  heatmap: {
    display: "grid",
    gap: 3,
    gridAutoFlow: "column",
    gridTemplateColumns: `repeat(${HEAT_WEEKS}, minmax(0,1fr))`,
    gridTemplateRows: "repeat(7, auto)",
  },
  heatCell: {
    aspectRatio: "1",
    backgroundColor: tokens.raised,
    borderRadius: 2,
    display: "block",
  },
  heatCellEmpty: { backgroundColor: tokens.raised },
  heat1: { backgroundColor: `color-mix(in oklch, ${tokens.accent} 30%, transparent)` },
  heat2: { backgroundColor: `color-mix(in oklch, ${tokens.accent} 50%, transparent)` },
  heat3: { backgroundColor: `color-mix(in oklch, ${tokens.accent} 72%, transparent)` },
  heat4: { backgroundColor: `color-mix(in oklch, ${tokens.accent} 100%, transparent)` },
  heatLegend: {
    alignItems: "center",
    color: tokens.faint,
    display: "flex",
    fontSize: 10,
    gap: 4,
    justifyContent: "flex-end",
    marginTop: 8,
  },
  legendCell: { borderRadius: 2, display: "block", height: 9, width: 9 },
  activityRow: {
    alignItems: "center",
    backgroundColor: { default: "transparent", ":hover": tokens.hover },
    borderBottomColor: tokens.subtleBorder,
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    borderWidth: 0,
    color: tokens.text,
    columnGap: 12,
    cursor: "pointer",
    display: "grid",
    fontSize: 13,
    gridTemplateColumns: "minmax(0,1fr) auto 40px",
    height: 32,
    paddingInline: 8,
    textAlign: "left",
    width: "100%",
  },
  showMore: {
    backgroundColor: "transparent",
    borderWidth: 0,
    color: { default: tokens.faint, ":hover": tokens.text },
    cursor: "pointer",
    fontSize: 12,
    padding: "10px 8px",
  },
  activitySide: {
    backgroundColor: tokens.elevated,
    borderColor: tokens.subtleBorder,
    borderRadius: 6,
    borderStyle: "solid",
    borderWidth: 1,
    display: { default: "flex", [PANEL_BELOW_WIDE]: "none" },
    flexDirection: "column",
    gap: 16,
    padding: 16,
  },
  sideTitle: { display: "block", fontSize: 13, fontWeight: 600 },
  sideMeta: { color: tokens.muted, fontSize: 12 },
  sideLabel: { color: tokens.faint, fontSize: 11 },
  barGroup: { display: "flex", flexDirection: "column", gap: 10 },
  barRow: { display: "flex", flexDirection: "column", gap: 5 },
  barName: { alignItems: "center", display: "flex", fontSize: 12, gap: 6 },
  barValue: { color: tokens.muted, fontVariantNumeric: "tabular-nums", marginLeft: "auto" },
  barTrack: {
    backgroundColor: tokens.raised,
    borderRadius: 2,
    display: "block",
    height: 3,
    overflow: "hidden",
  },
  barFill: { backgroundColor: tokens.muted, display: "block", height: "100%" },
  sideHint: { color: tokens.faint, fontSize: 12, margin: 0 },

  // Add skill sheet
  sheetBackdrop: {
    display: "flex",
    inset: 0,
    justifyContent: "flex-end",
    position: "absolute",
    zIndex: 8,
  },
  sheetScrim: {
    backgroundColor: tokens.scrim,
    borderWidth: 0,
    cursor: "default",
    inset: 0,
    padding: 0,
    position: "absolute",
  },
  sheet: {
    backgroundColor: tokens.background,
    borderLeftColor: tokens.border,
    borderLeftStyle: "solid",
    borderLeftWidth: 1,
    boxShadow: "-16px 0 48px oklch(0 0 0 / .3)",
    display: "flex",
    flexDirection: "column",
    maxWidth: "100%",
    position: "relative",
    width: 400,
  },
  sheetHeader: {
    alignItems: "center",
    display: "flex",
    justifyContent: "space-between",
    padding: "14px 12px 10px 20px",
  },
  sheetTitle: { fontSize: 14, fontWeight: 600, margin: 0 },
  sheetTabs: {
    borderBottomColor: tokens.border,
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    display: "flex",
    paddingInline: 16,
  },
  sheetTab: {
    backgroundColor: "transparent",
    borderBottomColor: "transparent",
    borderBottomStyle: "solid",
    borderBottomWidth: 2,
    borderWidth: 0,
    color: { default: tokens.muted, ":hover": tokens.text },
    cursor: "pointer",
    fontSize: 13,
    height: 36,
    paddingInline: 12,
  },
  sheetTabOn: { borderBottomColor: tokens.accent, color: tokens.text },
  sheetBody: {
    display: "flex",
    flex: 1,
    flexDirection: "column",
    gap: 18,
    minHeight: 0,
    overflow: "auto",
    padding: "16px 20px",
    scrollbarWidth: "none",
  },
  field: { display: "flex", flexDirection: "column", gap: 8 },
  textInput: {
    backgroundColor: "transparent",
    borderColor: { default: tokens.border, ":focus": tokens.accent },
    borderRadius: 4,
    borderStyle: "solid",
    borderWidth: 1,
    color: tokens.text,
    fontSize: 13,
    height: 32,
    outline: "none",
    paddingInline: 10,
    "::placeholder": { color: tokens.faint },
    [MOBILE]: { fontSize: 16 },
  },
  fieldHelp: { color: tokens.faint, fontSize: 11 },
  foundSkill: { alignItems: "center", display: "flex", fontSize: 13, gap: 8 },
  destination: {
    alignItems: "center",
    cursor: "pointer",
    display: "grid",
    gap: 10,
    gridTemplateColumns: "16px minmax(0,1fr) 16px",
    position: "relative",
  },
  destinationCopy: { display: "flex", flexDirection: "column", fontSize: 13, minWidth: 0 },
  destinationPath: { color: tokens.faint, fontFamily: monoStack, fontSize: 10 },
  checkboxInput: { gridColumn: 3, gridRow: 1, height: 16, margin: 0, opacity: 0, width: 16 },
  checkbox: {
    alignItems: "center",
    borderColor: tokens.strongBorder,
    borderRadius: 4,
    borderStyle: "solid",
    borderWidth: 1,
    color: "oklch(1 0 0)",
    display: "inline-flex",
    gridColumn: 3,
    gridRow: 1,
    height: 16,
    justifyContent: "center",
    pointerEvents: "none",
    width: 16,
  },
  checkboxOn: { backgroundColor: tokens.accentSolid, borderColor: tokens.accentSolid },
  checkboxCovered: { opacity: 0.5 },
  sheetFooter: {
    borderTopColor: tokens.border,
    borderTopStyle: "solid",
    borderTopWidth: 1,
    display: "flex",
    gap: 8,
    justifyContent: "flex-end",
    padding: "14px 20px",
  },

  // Compare dialog
  compareBackdrop: {
    alignItems: "center",
    backgroundColor: tokens.scrim,
    display: "flex",
    inset: 0,
    justifyContent: "center",
    position: "absolute",
    zIndex: 8,
  },
  compareDialog: {
    backgroundColor: tokens.background,
    borderColor: tokens.border,
    borderRadius: 10,
    borderStyle: "solid",
    borderWidth: 1,
    boxShadow: "0 24px 80px oklch(0 0 0 / .45)",
    display: "flex",
    flexDirection: "column",
    maxHeight: "85%",
    overflow: "hidden",
    width: "min(720px, 92%)",
  },
  compareHeader: {
    alignItems: "center",
    borderBottomColor: tokens.border,
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    display: "flex",
    justifyContent: "space-between",
    padding: "12px 12px 12px 20px",
  },
  comparePickers: {
    display: "grid",
    gap: 12,
    gridTemplateColumns: "1fr 1fr",
    padding: "12px 20px 0",
  },
  comparePicker: { justifyContent: "space-between", minWidth: 0, overflow: "hidden" },
  diff: {
    borderColor: tokens.border,
    borderRadius: 6,
    borderStyle: "solid",
    borderWidth: 1,
    fontFamily: monoStack,
    fontSize: 12,
    margin: 20,
    overflow: "hidden",
  },
  diffHeader: {
    backgroundColor: tokens.surface,
    borderBottomColor: tokens.border,
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    display: "grid",
    gap: 10,
    gridTemplateColumns: "1fr auto auto",
    padding: "8px 12px",
  },
  diffRow: {
    alignItems: "center",
    color: tokens.muted,
    display: "grid",
    gridTemplateColumns: "34px 34px minmax(0, 1fr)",
    minHeight: 24,
    paddingInline: 4,
  },
  diffRemoved: { backgroundColor: "oklch(0.645 0.208 25.331 / .14)", color: tokens.text },
  diffAdded: { backgroundColor: "oklch(0.696 0.149 162.48 / .14)", color: tokens.text },

  // Detail
  detailLayout: {
    alignItems: "start",
    display: "grid",
    gap: 32,
    gridTemplateColumns: { default: "minmax(0,1fr) 250px", [PANEL_BELOW_WIDE]: "minmax(0,1fr)" },
  },
  detailMain: { display: "flex", flexDirection: "column", gap: 20, minWidth: 0 },
  detailIntro: { alignItems: "flex-start", display: "flex", flexDirection: "column", gap: 6 },
  detailTitle: { fontSize: 18, fontWeight: 600, margin: 0 },
  detailSource: { color: tokens.faint, fontSize: 12 },
  detailDescription: { color: tokens.muted, fontSize: 13, lineHeight: 1.5, margin: "4px 0 0" },
  pill: {
    backgroundColor: tokens.accentSoft,
    borderRadius: 999,
    color: tokens.accent,
    fontSize: 11,
    marginTop: 4,
    maxWidth: "100%",
    overflow: "hidden",
    paddingBlock: 3,
    paddingInline: 8,
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  pillDanger: { backgroundColor: "oklch(0.645 0.208 25.331 / .12)", color: tokens.danger },
  pillWarning: { backgroundColor: "oklch(0.769 0.165 70.08 / .12)", color: tokens.warning },
  card: {
    borderColor: tokens.border,
    borderRadius: 10,
    borderStyle: "solid",
    borderWidth: 1,
    display: "flex",
    flexDirection: "column",
    gap: 6,
    padding: 16,
    [MOBILE]: { padding: 12 },
  },
  cardTop: { alignItems: "center", display: "flex", justifyContent: "space-between" },
  cardTopEnd: { alignItems: "center", display: "flex", fontSize: 11, gap: 8 },
  cardTitle: { fontSize: 13, fontWeight: 600, margin: "0 0 4px" },
  locationRow: {
    alignItems: "center",
    display: "grid",
    gap: 10,
    gridTemplateColumns: "minmax(0,1fr) auto auto auto",
    height: 34,
  },
  locationMain: {
    alignItems: "center",
    backgroundColor: "transparent",
    borderWidth: 0,
    color: tokens.text,
    cursor: "pointer",
    display: "flex",
    fontSize: 13,
    gap: 8,
    minWidth: 0,
    padding: 0,
    textAlign: "left",
  },
  locationIndent: { cursor: "default", paddingLeft: 22 },
  locationChevron: { color: tokens.faint, transition: "transform 150ms ease-out" },
  locationChevronOpen: { transform: "rotate(90deg)" },
  locationPath: {
    color: tokens.faint,
    display: "block",
    fontFamily: monoStack,
    fontSize: 11,
    paddingBottom: 6,
    paddingLeft: 45,
  },
  switchTrack: {
    backgroundColor: tokens.strongBorder,
    borderRadius: 999,
    borderWidth: 0,
    cursor: "pointer",
    display: "inline-flex",
    height: 16,
    padding: 2,
    transition: "background-color 150ms ease-out",
    width: 26,
  },
  switchTrackOn: { backgroundColor: tokens.accentSolid },
  switchKnob: {
    backgroundColor: "oklch(1 0 0)",
    borderRadius: "50%",
    display: "block",
    height: 12,
    transition: "transform 150ms ease-out",
    width: 12,
  },
  switchKnobOn: { transform: "translateX(10px)" },
  invocation: {
    borderTopColor: tokens.subtleBorder,
    borderTopStyle: "solid",
    borderTopWidth: 1,
    display: "flex",
    flexDirection: "column",
    gap: 8,
    marginTop: 8,
    paddingTop: 14,
  },
  invocationRow: {
    alignItems: "center",
    display: "flex",
    flexWrap: "wrap",
    gap: 10,
    justifyContent: "space-between",
  },
  invocationName: { alignItems: "center", display: "flex", fontSize: 13, gap: 8 },
  invocationNote: { color: tokens.faint, fontSize: 12, margin: 0 },
  markdown: { display: "flex", flexDirection: "column", gap: 8, padding: "8px 16px 4px" },
  markdownTitle: { fontSize: 17, fontWeight: 600, margin: 0 },
  markdownHeading: { fontSize: 13, fontWeight: 600, margin: "6px 0 0" },
  markdownParagraph: { color: tokens.muted, fontSize: 13, lineHeight: 1.55, margin: 0 },
  rail: {
    columnGap: 16,
    display: { default: "grid", [PANEL_BELOW_WIDE]: "none" },
    fontSize: 12,
    gridTemplateColumns: "64px minmax(0,1fr)",
    margin: 0,
    rowGap: 14,
  },
  railAgents: { alignItems: "center", display: "flex", gap: 6 },
  railMono: { display: "block", fontFamily: monoStack, fontSize: 11 },
  railSub: { color: tokens.faint, fontSize: 11 },
});
