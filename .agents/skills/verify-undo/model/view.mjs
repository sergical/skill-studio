export const title = "Undo of one change, when the app can die between two steps";

const undoing = (v) => v.job === "undo" || v.pc.startsWith("core_") || v.pc.startsWith("desk_");
const marked = (v) => v.revertedBy !== 0;
const finished = (v) => marked(v) && v.row[v.revertedBy] === "done";
const filesBack = (v) => v.files === "restored";
const stuck = (v) => marked(v) && !finished(v) && v.recovered && v.pc === "idle";

const resting = (v) => {
  if (finished(v)) return "Undone. Files are back";
  if (stuck(v)) return "Stuck: shown as undone for good. No undo finished, and Undo is refused";
  if (marked(v)) return "Shown as undone, but no undo finished";
  return filesBack(v)
    ? "Files are back, but Undo is still offered (it asks for Force)"
    : "Change in place. Undo offered";
};

const running = (v) => {
  if (v.cur === 0 || v.revertedBy !== v.cur) return "Undo started. Nothing of it shows yet";
  return filesBack(v)
    ? "Undo running: shown as undone, files back"
    : "Undo running: shown as undone, files not back";
};

export const state = (v) => (undoing(v) ? running(v) : resting(v));

export const group = (v) => {
  if (undoing(v)) return "undo running";
  return marked(v) === finished(v) && !(filesBack(v) && !marked(v)) ? "at rest" : "cut off";
};

export const colors = { "at rest": "teal", "undo running": "blue", "cut off": "yellow" };

const press = (_, to) => (to.force ? "you press Undo with Force" : "you press Undo");
const refusal = (from) =>
  marked(from)
    ? "Undo is refused: already undone"
    : "Undo is refused: files changed since, it asks for Force";

const labels = {
  YouAskCoreToUndo: press,
  YouAskDesktopToUndo: press,
  AnotherWriteStarts: () => "another write starts",
  RecoveryReleasesClaim: (from, to) => {
    if (from.revertedBy === to.revertedBy) return "the next write looks for a cut-off undo";
    return filesBack(to)
      ? "the next write clears the mark (the files are back)"
      : "the next write clears the mark (the files are not back)";
  },
  RecoveryMarksRowInterrupted: () => "the next write notes the cut-off undo",
  RecoveryDoneWithRows: () => "the next write goes on",
  RecoveryReversesPlan: (from, to) => {
    if (from.files !== to.files) return "the next write reverses the unsealed folder swap";
    return stuck(to)
      ? "the next write finds no undo record, so the mark stays"
      : "the next write finds nothing to clear";
  },
  CoreChecks: (from, to) => (to.pc === "idle" ? refusal(from) : "the checks pass"),
  CoreRecordsRestoreRow: () => "the undo is recorded",
  CoreClaims: () => "the change is marked undone",
  CoreSwapsFilesIn: () => "the files are swapped back",
  CoreClosesPlan: () => "the swap is sealed",
  CoreWriteFails: () => "a file write fails",
  CoreReleasesAfterFailure: () => "a file write failed: the mark is cleared",
  CoreFinishesFailed: () => "the undo is recorded as failed",
  CoreFinishesDone: () => "the undo is recorded as finished",
  DesktopChecksClaim: (from, to) => (to.pc === "idle" ? refusal(from) : "the checks pass"),
  DesktopClaims: () => "the change is marked undone",
  DesktopChecksDrift: () => "the files are checked",
  DesktopRecordsRestoreRow: () => "the undo is recorded",
  DesktopPutsFilesBack: () => "the files are put back",
  DesktopWriteFails: () => "a file write fails",
  DesktopFinishesFailed: () => "the undo is recorded as failed",
  DesktopReleasesAfterFailure: (from) =>
    from.row[from.cur] === "failed"
      ? "a file write failed: the mark is cleared"
      : "Undo is refused: files changed since, it asks for Force",
  DesktopFinishesDone: () => "the undo is recorded as finished",
  DesktopStartsUp: () => "the desktop app starts again",
  WriterDies: () => "the app dies",
};

export const step = (action, from, to) => (labels[action] ? labels[action](from, to) : action);
