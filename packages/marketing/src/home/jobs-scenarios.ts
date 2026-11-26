// ============================================================================
// Skill Studio - Interactive Fixer scenarios
// Five everyday multi-agent skill problems paired with the real manual checks
// they replace and Skill Studio's one-click resolution.
// ============================================================================

export interface Scenario {
  id: "ignored" | "unused" | "budget" | "update" | "share";
  question: string;
  painSummary: string;
  painCost: string;
  painSteps: ReadonlyArray<string>;
  painTerminal: ReadonlyArray<{ text: string; tone?: "cmd" | "err" | "warn" | "dim" }>;
  fixVerb: string;
  solutionHeadline: string;
  solutionSummary: string;
  solutionOutcome: string;
  command: string;
}

export const SCENARIOS: ReadonlyArray<Scenario> = [
  {
    id: "ignored",
    question: "“Why did Claude ignore my skill?”",
    painSummary:
      "You installed a skill, asked for it, and the agent ignored it. No error, just silence.",
    painCost: "4 folders · 5 manual checks",
    painSteps: [
      "Check ~/.claude/skills, ~/.codex/skills, ~/.agents/skills",
      "Inspect symlinks to see if the target repo moved",
      "Open SKILL.md to check broken YAML frontmatter",
      "Grep raw .jsonl session logs to see if it even loaded",
    ],
    painTerminal: [
      { text: "$ ls -la ~/.claude/skills/pdf-tools", tone: "cmd" },
      { text: "pdf-tools -> /Users/you/old-repo/pdf-tools  (broken)", tone: "err" },
      { text: "$ grep -rn 'writing-skills' ~/.claude/projects/", tone: "cmd" },
      { text: "(no description in prompt; cut by 1% budget?)", tone: "warn" },
      { text: "$ head -n 12 ~/.claude/skills/motion/SKILL.md", tone: "cmd" },
      { text: "# still guessing which session saw what…", tone: "dim" },
    ],
    fixVerb: "Inspect & fix in Sessions",
    solutionHeadline: "See what loaded, why it missed, and fix broken links in 1 click.",
    solutionSummary:
      "Sessions reads every agent’s transcript, flags name-only skills and broken headers, and repairs dead symlinks with undo.",
    solutionOutcome: "1 view · 1-click Fix",
    command: "npx skill-studio@latest sessions",
  },
  {
    id: "unused",
    question: "“Which skills are wasting my budget?”",
    painSummary:
      "Dozens of skills you tried once still eat listing budget every session, pushing the skills you actually need down to name-only.",
    painCost: "30 days of logs · 6 agent folders",
    painSteps: [
      "Parse session logs across Claude Code, Codex and Cursor",
      "Tally which skills loaded 90+ times with 0 runs",
      "Manually move folders out of every agent directory",
      "Remember where you stashed them if you need one back",
    ],
    painTerminal: [
      { text: "$ find ~/.claude/projects -name '*.jsonl' -mtime -30", tone: "cmd" },
      { text: "412 files found (no usage summary)", tone: "dim" },
      { text: "$ mv ~/.claude/skills/release-notes ~/skills-bak/", tone: "cmd" },
      { text: "$ mv ~/.codex/skills/release-notes ~/skills-bak/", tone: "cmd" },
      { text: "mv: rename failed: symlink in ~/.cursor/skills still points here", tone: "err" },
    ],
    fixVerb: "Park 3 idle skills",
    solutionHeadline: "Spot skills that never run and park them across every agent.",
    solutionSummary:
      "Usage ranks every skill by sessions listed vs. actually invoked. Park removes clutter cleanly across agents; Unpark puts it right back.",
    solutionOutcome: "+70 descriptions freed · Undo",
    command: "npx skill-studio@latest usage",
  },
  {
    id: "budget",
    question: "“Will raising my budget actually fix it?”",
    painSummary:
      "Changing Claude’s listing fraction or Codex’s token cap is blind guesswork until you restart and run a real session.",
    painCost: "Edit JSON · Restart · Guess",
    painSteps: [
      "Run wc -c across every installed SKILL.md description",
      "Hand-edit ~/.claude/settings.json or ~/.codex/config.toml",
      "Start a fresh session just to test if key skills fit",
      "Repeat until the skills you care about keep descriptions",
    ],
    painTerminal: [
      { text: "$ wc -c ~/.claude/skills/*/SKILL.md | tail -n 1", tone: "cmd" },
      { text: "  48,920 total  (which ones get cut at 1%?)", tone: "warn" },
      { text: "$ vim ~/.claude/settings.json", tone: "cmd" },
      { text: '  "skillListingBudgetFraction": 0.02  // try 2%?', tone: "dim" },
      { text: "# restart Claude Code and hope writing-skills made the cut", tone: "dim" },
    ],
    fixVerb: "Preview 2% budget diff",
    solutionHeadline: "See which exact skills gain descriptions before touching settings.",
    solutionSummary:
      "Model listing simulates your real library at any context window or budget, then hands you the exact setting line and file path.",
    solutionOutcome: "Exact diff · Zero guesswork",
    command: "npx skill-studio@latest listing",
  },
  {
    id: "update",
    question: "“Which agent has the stale copy?”",
    painSummary:
      "Claude Code has Tuesday’s skill, Codex has last month’s, and running a blind update risks clobbering the skill you tweaked locally.",
    painCost: "Diff 6 folders · Risk local edits",
    painSteps: [
      "Diff ~/.claude/skills against ~/.codex/skills & ~/.cursor/skills",
      "Check which upstream repos pushed new versions",
      "Remember which SKILL.md files you edited by hand",
      "Update one by one so you don’t overwrite local tweaks",
    ],
    painTerminal: [
      { text: "$ diff -rq ~/.claude/skills ~/.codex/skills", tone: "cmd" },
      { text: "Files react-best-practices/SKILL.md differ (v1.4 vs v1.7)", tone: "warn" },
      { text: "$ npx skills update", tone: "cmd" },
      { text: "WARNING: local changes in deploy-preview may be overwritten", tone: "err" },
    ],
    fixVerb: "Update safe copies now",
    solutionHeadline: "Update every agent together without touching your local edits.",
    solutionSummary:
      "Home flags outdated skills across every linked folder, updates clean copies in one click, and pauses on anything you edited locally.",
    solutionOutcome: "4 agents synced · Local edits safe",
    command: "npx skill-studio@latest outdated",
  },
  {
    id: "share",
    question: "“How do I share my skills with teammates?”",
    painSummary:
      "Onboarding a teammate means pasting a dozen repo links into Slack and hoping they put every folder in the right agent directory.",
    painCost: "14 URLs in Slack · Manual sync",
    painSteps: [
      "Dump ~/.agents/.skill-lock.json and clean out personal skills",
      "Paste 14 git URLs + install commands into Notion or Slack",
      "Walk teammates through Claude, Codex and Cursor paths",
      "Ping the channel every time a skill updates upstream",
    ],
    painTerminal: [
      { text: "$ cat ~/.agents/.skill-lock.json | grep url", tone: "cmd" },
      { text: "14 repos + 3 custom local folders", tone: "dim" },
      { text: "# paste 14 install lines into #eng-onboarding", tone: "warn" },
      { text: "# “hey, which folder does Codex read again?”", tone: "err" },
    ],
    fixVerb: "Preview acme/skills pack",
    solutionHeadline: "Share one git repo. Teammates preview and install in one click.",
    solutionSummary:
      "Packs turns any git repo into a curated skill bundle with upstream update checks that respect each teammate’s local edits.",
    solutionOutcome: "1 git URL · Team-wide updates",
    command: "npx skill-studio@latest pack add acme/skills",
  },
];
