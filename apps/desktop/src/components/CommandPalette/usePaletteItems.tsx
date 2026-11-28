// ============================================================================
// usePaletteItems - Everything the command palette can run: the needs-attention
// skills, the actions, the places the sidebar shows, and every skill by name.
// ============================================================================

import {
  ArrowLeft,
  ArrowRight,
  BookOpen,
  Bot,
  Boxes,
  Layers,
  LayoutDashboard,
  MessagesSquare,
  Moon,
  PackageOpen,
  Plus,
  Puzzle,
  RefreshCw,
  Search,
  Settings as SettingsIcon,
  Sun,
  Activity as ActivityIcon,
} from "lucide-react";
import {
  applySkillListFilter,
  attentionBySkill,
  collectDashboardIssues,
  defaultSkillListFilter,
  ownSkillsView,
  ownSkillsWithPluginUpdates,
  pluginSkillsView,
} from "@skill-studio/lib";
import type { InstalledSkill, SkillSnapshot } from "@skill-studio/lib";
import { SHORTCUTS } from "../../lib/app-shortcuts";
import { groupSkillRows } from "../../lib/skill-list-model";
import { useAppStore } from "../../store/appStore";
import { RowGlyph } from "../SkillList/SkillRowCells";
import { ROW_GROUP_LABEL, rowGroup, rowState } from "../SkillList/skill-row-state";
import { gotoItems, skillItems } from "./command-palette-items";
import type { PaletteItem } from "./command-palette-items";

/** Skills listed under Needs attention before a "Show all" row takes over. */
const ATTENTION_PREVIEW_COUNT = 5;

/** The palette's items, unranked: needs attention, actions, places, then skills. */
export function usePaletteItems(
  snapshot: SkillSnapshot | undefined,
  requestRescan: () => Promise<void>,
): PaletteItem[] {
  const setActiveView = useAppStore((state) => state.setActiveView);
  const openSkill = useAppStore((state) => state.openSkill);
  const openAddSkillSheet = useAppStore((state) => state.openAddSkillSheet);
  const requestSkillSearchFocus = useAppStore((state) => state.requestSkillSearchFocus);
  const resolvedTheme = useAppStore((state) => state.resolvedTheme);
  const addToast = useAppStore((state) => state.addToast);
  const setTheme = useAppStore((state) => state.setTheme);
  const goBack = useAppStore((state) => state.goBack);
  const skillListFilter = useAppStore((state) => state.skillListFilter);
  const replaceSkillListFilter = useAppStore((state) => state.replaceSkillListFilter);
  const goForward = useAppStore((state) => state.goForward);

  const actions: PaletteItem[] = [
    {
      id: "action-add-skill",
      section: "actions",
      label: SHORTCUTS.addSkill.label,
      icon: <Plus size={14} />,
      shortcut: SHORTCUTS.addSkill,
      run: () => openAddSkillSheet(),
    },
    {
      id: "action-back",
      section: "actions",
      label: SHORTCUTS.back.label,
      icon: <ArrowLeft size={14} />,
      shortcut: SHORTCUTS.back,
      typedOnly: true,
      run: () => goBack(),
    },
    {
      id: "action-forward",
      section: "actions",
      label: SHORTCUTS.forward.label,
      icon: <ArrowRight size={14} />,
      shortcut: SHORTCUTS.forward,
      typedOnly: true,
      run: () => goForward(),
    },
    {
      id: "action-sync",
      section: "actions",
      label: SHORTCUTS.sync.label,
      icon: <RefreshCw size={14} />,
      run: () => {
        addToast({
          type: "info",
          title: "Rescanning skills…",
          message: "The list updates when it finishes.",
        });
        requestRescan().catch((err) =>
          addToast({
            type: "error",
            title: "Couldn't rescan",
            message: err instanceof Error ? err.message : "Unknown error",
          }),
        );
      },
    },
    {
      id: "action-filter-skills",
      section: "actions",
      label: "Filter skills",
      icon: <Search size={14} />,
      shortcut: SHORTCUTS.filterSkills,
      run: () => {
        setActiveView({ kind: "skills" });
        requestSkillSearchFocus();
      },
    },
    {
      id: "action-toggle-theme",
      section: "actions",
      label: SHORTCUTS.toggleTheme.label,
      icon: resolvedTheme === "dark" ? <Sun size={14} /> : <Moon size={14} />,
      run: () => setTheme(resolvedTheme === "dark" ? "light" : "dark"),
    },
  ];

  const allSkills = snapshot?.skills ?? [];
  const own = ownSkillsView(allSkills);
  const parkedCount = own.filter((skill) => skill.parked).length;
  const pluginCount = pluginSkillsView(allSkills).length;
  // The places the sidebar shows, in its order: Plugins and Parked only when they hold skills.
  const goto = gotoItems([
    {
      id: "home",
      label: "Home",
      icon: <LayoutDashboard size={14} />,
      run: () => setActiveView({ kind: "home" }),
    },
    {
      id: "skills",
      label: "Skills",
      icon: <Layers size={14} />,
      run: () => {
        if (skillListFilter.scope === "parked") replaceSkillListFilter({});
        setActiveView({ kind: "skills" });
      },
    },
    ...(pluginCount > 0
      ? [
          {
            id: "plugins",
            label: "Plugins",
            icon: <Puzzle size={14} />,
            run: () => setActiveView({ kind: "plugins" }),
          },
        ]
      : []),
    {
      id: "packs",
      label: "Packs",
      icon: <Boxes size={14} />,
      run: () => setActiveView({ kind: "packs" }),
    },
    {
      id: "listing",
      label: "Agents › What it sees",
      icon: <Bot size={14} />,
      run: () => setActiveView({ kind: "listing" }),
    },
    {
      id: "sessions",
      label: "Agents › What it used",
      icon: <MessagesSquare size={14} />,
      run: () => setActiveView({ kind: "sessions" }),
    },
    {
      id: "activity",
      label: "Activity",
      icon: <ActivityIcon size={14} />,
      run: () => setActiveView({ kind: "activity" }),
    },
    ...(parkedCount > 0
      ? [
          {
            id: "parked",
            label: "Parked",
            icon: <PackageOpen size={14} />,
            run: () => {
              replaceSkillListFilter({ scope: "parked" });
              setActiveView({ kind: "skills" });
            },
          },
        ]
      : []),
    {
      id: "learn",
      label: "Learn",
      icon: <BookOpen size={14} />,
      run: () => setActiveView({ kind: "learn" }),
    },
    {
      id: "settings",
      label: "Settings",
      icon: <SettingsIcon size={14} />,
      run: () => setActiveView({ kind: "settings" }),
    },
  ]);

  // The same categories, skills and order as the Skills page's Needs attention filter (its own
  // grouping at its default sort), so a count here matches the one there.
  const issues = collectDashboardIssues(own);
  const withPluginUpdates = ownSkillsWithPluginUpdates(allSkills);
  const attention = attentionBySkill(withPluginUpdates, issues);
  const groupOf = (skill: InstalledSkill) => rowGroup(skill, attention.get(skill.name));
  const groupLabel = (skill: InstalledSkill) => {
    const group = groupOf(skill);
    return group === "healthy" ? undefined : ROW_GROUP_LABEL[group];
  };
  const glyph = (skill: InstalledSkill) => {
    const state = rowState(skill);
    return state && <RowGlyph state={state} size={14} />;
  };
  const needsAttention = groupSkillRows(
    applySkillListFilter(
      withPluginUpdates,
      { ...defaultSkillListFilter(), attention: "any" },
      issues,
    ),
    "name",
    new Map(),
    attention,
  ).rows;
  const attentionItems = skillItems(
    needsAttention.slice(0, ATTENTION_PREVIEW_COUNT),
    "attention",
    groupLabel,
    glyph,
    (skill) => openSkill(skill.name),
  );
  if (needsAttention.length > ATTENTION_PREVIEW_COUNT)
    attentionItems.push({
      id: "attention-all",
      section: "attention",
      label: `Show all ${needsAttention.length}`,
      icon: <ArrowRight size={14} />,
      run: () => {
        replaceSkillListFilter({ attention: "any" });
        setActiveView({ kind: "skills" });
      },
    });

  const skills = skillItems(allSkills, "skills", groupLabel, glyph, (skill) =>
    openSkill(skill.name),
  );

  return [...attentionItems, ...actions, ...goto, ...skills];
}
