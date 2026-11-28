// ============================================================================
// PasteSkillTab - the Add skill sheet's Paste tab: a new skill from pasted
// text, such as a SKILL.md copied from Slack or a gist. The backend builds the
// file and runs the scanner's own validator on every change
// (`check_pasted_skill`), so the tab shows the name, description and problems
// before anything is written. Add skill writes it as a Global Universal Copy
// (`create_pasted_skill`), which refuses a folder that already exists.
// ============================================================================

import { useEffect, useState } from "react";
import { Button, Input, Textarea } from "@skill-studio/ui";
import { describeSpecViolations } from "@skill-studio/lib";
import type { InstalledSkill, PastedSkillCheck, PastedSkillDraft } from "@skill-studio/lib";
import { isInstalledIn } from "../../lib/skill-scope-model";
import { useAppStore } from "../../store/appStore";

const LABEL_CLASS = "text-small font-medium text-text-secondary";
const FIELD_CLASS =
  "rounded-sm border-border bg-bg-primary text-body text-text-primary focus-visible:border-border-focus focus-visible:ring-0";

const EMPTY_DRAFT: PastedSkillDraft = { text: "", name: "", description: "" };

interface PasteSkillTabProps {
  skills: readonly InstalledSkill[];
  onCancel: () => void;
  /** Called with the new skill's name once it is on disk. */
  onAdded: (name: string) => void;
}

/** The backend's answer for the draft on screen. A stale answer still shows
 * while the next one is on its way, but cannot be saved. */
function useDraftCheck(draft: PastedSkillDraft) {
  const checkPastedSkill = useAppStore((state) => state.checkPastedSkill);
  const [checked, setChecked] = useState<{
    draft: PastedSkillDraft;
    result: PastedSkillCheck | null;
    error: string | null;
  } | null>(null);
  useEffect(() => {
    if (!draft.text.trim()) return;
    let cancelled = false;
    const run = async () => {
      try {
        const result = await checkPastedSkill(draft);
        if (!cancelled) setChecked({ draft, result, error: null });
      } catch (err) {
        const error = err instanceof Error ? err.message : "Could not check this skill";
        if (!cancelled) setChecked({ draft, result: null, error });
      }
    };
    void run();
    return () => {
      cancelled = true;
    };
  }, [checkPastedSkill, draft]);
  if (!draft.text.trim() || !checked) return { check: null, current: false, error: null };
  return { check: checked.result, current: checked.draft === draft, error: checked.error };
}

export function PasteSkillTab({ skills, onCancel, onAdded }: PasteSkillTabProps) {
  const addToast = useAppStore((state) => state.addToast);
  const createPastedSkill = useAppStore((state) => state.createPastedSkill);
  const [draft, setDraft] = useState<PastedSkillDraft>(EMPTY_DRAFT);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const { check, current, error: checkError } = useDraftCheck(draft);

  const name = check?.name ?? null;
  const taken = name !== null && isInstalledIn(skills, name, "global");
  const canSave = current && check?.can_save === true && !taken && !saving;
  // Without frontmatter the Name and Description fields ask for the missing fields themselves.
  const shownProblems = (check?.problems ?? []).filter(
    (problem) =>
      check?.has_frontmatter !== false || !problem.startsWith("missing required frontmatter field"),
  );
  const problems = shownProblems.length > 0 ? describeSpecViolations(shownProblems) : null;

  const update = (patch: Partial<PastedSkillDraft>) => {
    setSaveError(null);
    setDraft((previous) => ({ ...previous, ...patch }));
  };

  const handleSave = async () => {
    if (!canSave) return;
    setSaving(true);
    setSaveError(null);
    try {
      const result = await createPastedSkill(draft);
      addToast({
        type: result.warning ? "warning" : "success",
        title: `Added ${result.name}`,
        message: result.warning ?? undefined,
      });
      onAdded(result.name);
    } catch (err) {
      setSaveError(err instanceof Error ? err.message : "Could not add this skill");
    } finally {
      setSaving(false);
    }
  };

  const saveOnModEnter = (event: React.KeyboardEvent<HTMLElement>) => {
    if (event.key !== "Enter" || !(event.metaKey || event.ctrlKey)) return;
    if (event.nativeEvent.isComposing) return;
    event.preventDefault();
    void handleSave();
  };

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      <div className="flex flex-1 flex-col gap-5 overflow-y-scroll py-4 pl-5 gutter-pr-5">
        <div className="flex flex-col gap-2">
          <label htmlFor="add-skill-paste" className={LABEL_CLASS}>
            SKILL.md
          </label>
          <Textarea
            id="add-skill-paste"
            className={`max-h-96 min-h-48 font-mono text-small ${FIELD_CLASS}`}
            value={draft.text}
            onChange={(e) => update({ text: e.target.value })}
            onKeyDown={saveOnModEnter}
            placeholder={
              "---\nname: my-skill\ndescription: What it does and when to use it\n---\n\nInstructions"
            }
            spellCheck={false}
            autoCorrect="off"
            autoCapitalize="off"
            aria-describedby="add-skill-paste-hint"
          />
          <p id="add-skill-paste-hint" className="m-0 text-small text-text-tertiary">
            Paste a whole SKILL.md, or just the instructions and fill in a name and description.
          </p>
        </div>

        {check && !check.has_frontmatter && (
          <>
            <div className="flex flex-col gap-2">
              <label htmlFor="add-skill-paste-name" className={LABEL_CLASS}>
                Name
              </label>
              <Input
                id="add-skill-paste-name"
                type="text"
                className={`h-(--control-height) ${FIELD_CLASS}`}
                value={draft.name}
                onChange={(e) => update({ name: e.target.value })}
                onKeyDown={saveOnModEnter}
                placeholder="my-skill"
                spellCheck={false}
                autoCorrect="off"
                autoCapitalize="off"
              />
            </div>
            <div className="flex flex-col gap-2">
              <label htmlFor="add-skill-paste-description" className={LABEL_CLASS}>
                Description
              </label>
              <Input
                id="add-skill-paste-description"
                type="text"
                className={`h-(--control-height) ${FIELD_CLASS}`}
                value={draft.description}
                onChange={(e) => update({ description: e.target.value })}
                onKeyDown={saveOnModEnter}
                placeholder="What it does and when to use it"
              />
            </div>
          </>
        )}

        <div className="flex flex-col gap-1 empty:hidden" role="status">
          {checkError && <p className="m-0 text-small text-error">{checkError}</p>}
          {taken && (
            <p className="m-0 text-small text-error">
              A skill named {name} is already installed. Change the name to add this one.
            </p>
          )}
          {problems && (
            <p className={`m-0 text-small ${check?.can_save ? "text-warning" : "text-error"}`}>
              {problems}
            </p>
          )}
        </div>

        {name && check?.can_save && !taken && (
          <p className="m-0 text-small text-text-tertiary">
            Adds {name} as a Global skill in <span className="font-mono">~/.agents/skills</span>
          </p>
        )}
      </div>

      <div className="flex flex-col gap-2 border-t border-border px-5 py-4">
        {saveError && (
          <p className="m-0 text-small text-error" role="alert">
            {saveError}
          </p>
        )}
        <div className="flex justify-end gap-2">
          <Button
            variant="outline"
            className="h-(--control-height) rounded-md px-3.5 text-body font-medium"
            onClick={onCancel}
          >
            Cancel
          </Button>
          <Button
            className="h-(--control-height) rounded-md bg-accent-solid px-3.5 text-body font-medium text-text-on-accent hover:bg-accent-solid-hover"
            onClick={() => void handleSave()}
            busy={saving}
            disabled={!canSave}
          >
            Add skill
          </Button>
        </div>
      </div>
    </div>
  );
}
