import { memo, useMemo, useState } from "react";
import * as stylex from "@stylexjs/stylex";
import { Check, Copy } from "lucide-react";

import { AgentIcon } from "../AgentIcon";
import { productDemoLightTheme } from "../ProductDemoTokens.stylex";
import type { SiteTheme } from "../SiteTheme.stylex";
import { BudgetSlider } from "./BudgetSlider";
import { homeSectionStyles } from "./HomeSection.stylex";
import {
  CLAUDE_DEFAULT_PERCENT,
  CLAUDE_DESCRIPTION_CAP,
  CLAUDE_EXPERIMENT_CAP,
  CLAUDE_MODELS,
  CODEX_DESCRIPTION_CAP,
  CODEX_MODELS,
  CODEX_OVERRIDE_CAP,
  MAX_DEMO_SKILLS,
  estimateExampleListing,
  type ListingState,
} from "./listing-budget-model";
import { listingBudgetStyles as styles } from "./ListingBudgetCalculator.stylex";
import { useCopyFlash } from "./useCopyFlash";

const LISTING_COMMAND = "npx -y skill-studio@latest listing";
const SAMPLE_DESCRIPTION =
  "Review a pull request for changes that could break normal use. Compare the diff with the request, run relevant checks, and explain each finding with a file and line number. ";
const FORMATS = new Intl.NumberFormat("en-US");
const STATE_LABELS = {
  full: "Full description",
  shortened: "Shortened",
  nameOnly: "Name only",
  dropped: "Dropped",
} as const;
const LISTING_STATES = ["full", "shortened", "nameOnly", "dropped"] as const;
const GRID_SLOTS = Array.from({ length: MAX_DEMO_SKILLS }, (_, index) => index);
const ACTUAL_INSTALL_COUNT = 438;

interface BudgetControls {
  agent: "claude" | "codex";
  skillCount: number;
  descriptionLength: number;
  claudeModel: number;
  codexModel: number;
  claudePercent: number;
  codexCustom: boolean;
  codexTokens: number;
}

const INITIAL_CONTROLS: BudgetControls = {
  agent: "claude",
  skillCount: 120,
  descriptionLength: 260,
  claudeModel: 0,
  codexModel: 0,
  claudePercent: CLAUDE_DEFAULT_PERCENT,
  codexCustom: false,
  codexTokens: CODEX_OVERRIDE_CAP,
};

const format = (value: number) => FORMATS.format(value);

function formatContext(tokens: number | null) {
  if (tokens === null) return "";
  return tokens === 1_000_000 ? "1M" : `${tokens / 1_000}k`;
}

function describeResult(
  agent: BudgetControls["agent"],
  total: number,
  counts: Record<ListingState, number>,
) {
  if (counts.full === total) return `All ${format(total)} descriptions fit.`;
  if (counts.dropped) {
    return `Codex leaves ${format(counts.dropped)} skills out of the list. The rest keep their names and paths.`;
  }
  if (counts.nameOnly) {
    return `${format(counts.nameOnly)} skills appear by name without a description.`;
  }
  return `${agent === "codex" ? "Codex" : "Claude Code"} shortens ${format(counts.shortened)} of ${format(total)} descriptions.`;
}

const SkillBudgetChip = memo(function SkillBudgetChip({ state }: { state?: ListingState }) {
  return <span {...stylex.props(styles.chip, state ? styles[state] : styles.empty)} />;
});

function ListingCommand() {
  const { copied, copy } = useCopyFlash();
  return (
    <button
      type="button"
      aria-label={copied ? "Listing command copied" : "Copy listing command"}
      onClick={() => copy(LISTING_COMMAND)}
      {...stylex.props(styles.command)}
    >
      <code {...stylex.props(homeSectionStyles.code)}>{LISTING_COMMAND}</code>
      {copied ? <Check size={15} aria-hidden="true" /> : <Copy size={15} aria-hidden="true" />}
    </button>
  );
}

/** Lets visitors experiment with how Claude Code and Codex fit skills into a model’s listing. */
export function ListingBudgetCalculator({ theme }: { theme: SiteTheme }) {
  const [controls, setControls] = useState(INITIAL_CONTROLS);
  function change<Key extends keyof BudgetControls>(key: Key, value: BudgetControls[Key]) {
    setControls((current) => (current[key] === value ? current : { ...current, [key]: value }));
  }

  const description = useMemo(
    () =>
      SAMPLE_DESCRIPTION.repeat(Math.ceil(controls.descriptionLength / SAMPLE_DESCRIPTION.length))
        .slice(0, controls.descriptionLength)
        .replace(/ $/, "."),
    [controls.descriptionLength],
  );
  const result = useMemo(
    () =>
      estimateExampleListing({
        agent: controls.agent,
        skillCount: controls.skillCount,
        description,
        claudeModel: CLAUDE_MODELS[controls.claudeModel],
        claudePercent: controls.claudePercent,
        codexWindow: CODEX_MODELS[controls.codexModel].tokens,
        codexOverride: controls.codexCustom ? controls.codexTokens : null,
      }),
    [controls, description],
  );
  const isClaude = controls.agent === "claude";
  const models = isClaude ? CLAUDE_MODELS : CODEX_MODELS;
  const model = isClaude ? controls.claudeModel : controls.codexModel;
  const extent = Math.max(result.limit, result.needed);
  const within = Math.min(result.limit, result.needed) / extent;
  const spilling = result.needed > result.limit;

  return (
    <>
      <h2 {...stylex.props(homeSectionStyles.title, styles.title)}>How many skills fit?</h2>
      <p {...stylex.props(styles.lede)}>
        An agent has limited room to list skill names and descriptions. Change the inputs to see
        what it would show the model.
      </p>
      <div {...stylex.props(styles.calculator, theme === "light" && productDemoLightTheme)}>
        <div {...stylex.props(styles.controls)}>
          <div role="group" aria-label="Agent" {...stylex.props(styles.agentChoices)}>
            <button
              type="button"
              aria-pressed={isClaude}
              onClick={() => change("agent", "claude")}
              {...stylex.props(styles.choice, isClaude && styles.selected)}
            >
              <span aria-hidden="true">
                <AgentIcon agent="claude" size={17} />
              </span>
              Claude Code
            </button>
            <button
              type="button"
              aria-pressed={!isClaude}
              onClick={() => change("agent", "codex")}
              {...stylex.props(styles.choice, !isClaude && styles.selected)}
            >
              <span aria-hidden="true">
                <AgentIcon agent="codex" size={17} />
              </span>
              Codex
            </button>
          </div>
          <p {...stylex.props(styles.note)}>
            Cursor, OpenCode, pi, and Grok Build have no known listing limit.
          </p>
          <div>
            <div {...stylex.props(styles.modelLabel)}>Model context window</div>
            <div
              role="group"
              aria-label="Model context window"
              {...stylex.props(styles.modelChoices)}
            >
              {models.map(({ label, tokens }, index) => (
                <button
                  key={label}
                  type="button"
                  aria-pressed={index === model}
                  onClick={() => change(isClaude ? "claudeModel" : "codexModel", index)}
                  {...stylex.props(styles.choice, index === model && styles.selected)}
                >
                  {label}
                  {tokens && <span {...stylex.props(styles.window)}>{formatContext(tokens)}</span>}
                </button>
              ))}
            </div>
          </div>
          <div>
            <BudgetSlider
              label="Skills in the list"
              value={controls.skillCount}
              valueText={format(controls.skillCount)}
              min={10}
              max={MAX_DEMO_SKILLS}
              onChange={(value) => change("skillCount", value)}
            />
            <button
              type="button"
              onClick={() => change("skillCount", ACTUAL_INSTALL_COUNT)}
              {...stylex.props(styles.preset)}
            >
              Try 438 installed skills
            </button>
          </div>
          <div>
            <BudgetSlider
              label="Average description length"
              value={controls.descriptionLength}
              valueText={`${format(controls.descriptionLength)} characters`}
              min={40}
              max={1_800}
              step={5}
              onChange={(value) => change("descriptionLength", value)}
            />
            <p tabIndex={0} aria-label="Example skill description" {...stylex.props(styles.sample)}>
              {description}
            </p>
          </div>
          <div {...stylex.props(styles.budgetControl)}>
            {isClaude ? (
              <>
                <BudgetSlider
                  label="Share of context for skills"
                  value={controls.claudePercent}
                  valueText={`${format(controls.claudePercent)}%`}
                  min={0.01}
                  max={100}
                  step={0.01}
                  curve={3}
                  onChange={(value) => change("claudePercent", value)}
                />
                <p {...stylex.props(styles.note)}>
                  Claude Code defaults to 1% of context at 3 or 4 characters per token. The setting
                  accepts more than 0% through 100%.
                </p>
                <p {...stylex.props(styles.note)}>
                  An account experiment can raise the default, up to {format(CLAUDE_EXPERIMENT_CAP)}{" "}
                  characters. Descriptions are capped at {format(CLAUDE_DESCRIPTION_CAP)} characters
                  by default.
                </p>
              </>
            ) : (
              <>
                <button
                  type="button"
                  aria-pressed={controls.codexCustom}
                  onClick={() => change("codexCustom", !controls.codexCustom)}
                  {...stylex.props(styles.choice, controls.codexCustom && styles.selected)}
                >
                  Use a custom token budget
                </button>
                {controls.codexCustom && (
                  <BudgetSlider
                    label="Custom token budget"
                    value={controls.codexTokens}
                    valueText={`${format(controls.codexTokens)} tokens`}
                    min={1}
                    max={CODEX_OVERRIDE_CAP}
                    curve={2}
                    onChange={(value) => change("codexTokens", value)}
                  />
                )}
                <p {...stylex.props(styles.note)}>
                  Codex uses 2% of the active model’s context or 8,000 characters if it’s unknown. A
                  custom budget takes 1 to {format(CODEX_OVERRIDE_CAP)} tokens. Each description has
                  a {format(CODEX_DESCRIPTION_CAP)}-character cap.
                </p>
              </>
            )}
          </div>
        </div>
        <div {...stylex.props(styles.results)}>
          <div {...stylex.props(styles.measurements)}>
            <div>
              <span {...stylex.props(styles.field)}>Listing budget</span>
              <strong {...stylex.props(styles.amount)}>{format(result.limit)}</strong>{" "}
              <span {...stylex.props(styles.unit)}>{result.unit}</span>
            </div>
            <div>
              <span {...stylex.props(styles.field)}>Needed before budget cuts</span>
              <strong {...stylex.props(styles.amount)}>{format(result.needed)}</strong>{" "}
              <span {...stylex.props(styles.unit)}>{result.unit}</span>
            </div>
          </div>
          <div aria-hidden="true" {...stylex.props(styles.bar, spilling && styles.overflow)}>
            <span {...stylex.props(styles.barFill, styles.scale(within))} />
            {spilling && <span {...stylex.props(styles.boundary, styles.boundaryAt(within))} />}
          </div>
          <h3 {...stylex.props(styles.resultHeading)}>What the model sees</h3>
          <dl {...stylex.props(styles.legend)}>
            {LISTING_STATES.map((state) => (
              <div key={state} {...stylex.props(styles.legendItem)}>
                <dt {...stylex.props(styles.legendLabel)}>
                  <span aria-hidden="true" {...stylex.props(styles.key, styles[state])} />
                  {STATE_LABELS[state]}
                </dt>
                <dd {...stylex.props(styles.count)}>{format(result.counts[state])}</dd>
              </div>
            ))}
          </dl>
          <div aria-hidden="true" {...stylex.props(styles.grid)}>
            {GRID_SLOTS.map((index) => (
              <SkillBudgetChip key={index} state={result.states[index]} />
            ))}
          </div>
          <p role="status" {...stylex.props(styles.outcome)}>
            {describeResult(controls.agent, controls.skillCount, result.counts)}
          </p>
          <p {...stylex.props(styles.note)}>
            This estimates one agent’s list using example names, paths, and descriptions. Skill
            Studio measures your real list.
          </p>
          <ListingCommand />
        </div>
      </div>
    </>
  );
}
