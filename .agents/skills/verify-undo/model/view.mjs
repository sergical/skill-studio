export const title = "Undo of one change, when the app can die between two steps";

const undoing = (v) => v.job === "undo" || v.pc.startsWith("core_") || v.pc.startsWith("desk_");
const marked = (v) => v.revertedBy !== 0;
const finished = (v) => marked(v) && v.row[v.revertedBy] === "done";
const stuck = (v) => marked(v) && !finished(v) && v.recovered && v.pc === "idle";
const filesAre = (v) =>
  ({ changed: "files not back", partial: "files half put back", restored: "files back" })[v.files];

const resting = (v) => {
  if (finished(v)) return "Undone. Files are back";
  if (stuck(v)) return "Stuck: shown as undone for good. No undo finished, and Undo is refused";
  if (marked(v)) return "Shown as undone, but no undo finished";
  if (v.files === "restored")
    return "Files are back, but Undo is still offered (it asks for Force)";
  if (v.files === "partial")
    return "Files are half put back. Undo is still offered (it asks for Force)";
  return "Change in place. Undo offered";
};

const running = (v) =>
  v.cur === 0 || v.revertedBy !== v.cur
    ? "Undo started. Nothing of it shows yet"
    : `Undo running: shown as undone, ${filesAre(v)}`;

export const state = (v) => (undoing(v) ? running(v) : resting(v));

export const group = (v) => {
  if (undoing(v)) return "undo running";
  return marked(v) === finished(v) && (marked(v) || v.files === "changed") ? "at rest" : "cut off";
};

export const colors = { "at rest": "teal", "undo running": "blue", "cut off": "yellow" };

const press = (_, to) => (to.force ? "you press Undo with Force" : "you press Undo");
const refusal = (from) =>
  marked(from)
    ? "Undo is refused: already undone"
    : "Undo is refused: files changed since, it asks for Force";
const who = (v) => (v.job === "start" ? "the desktop app starts again and" : "the next write");

const labels = {
  YouAskCoreToUndo: press,
  YouAskDesktopToUndo: press,
  AnotherWriteStarts: () => "another write starts",
  DesktopAppStarts: () => "the desktop app starts again",
  RecoveryReleasesClaim: (from, to) =>
    from.revertedBy === to.revertedBy
      ? "the next write looks for a cut-off undo"
      : `the next write clears the mark (${filesAre(to)})`,
  RecoveryMarksRowInterrupted: (from) => `${who(from)} notes the cut-off undo`,
  RecoveryDoneWithRows: (from) => `${who(from)} goes on`,
  RecoveryStartsPuttingFolderBack: (from) =>
    `${who(from)} starts to reverse the unsealed folder swap`,
  RecoveryPutsSwappedFolderBack: (from) => `${who(from)} reverses the unsealed folder swap`,
  RecoveryClosesPlan: (from) => `${who(from)} seals the reversal`,
  RecoveryEnds: (from, to) =>
    stuck(to)
      ? "the next write finds no undo record, so the mark stays"
      : `${who(from)} is done recovering`,
  CoreChecks: (from, to) => (to.pc === "idle" ? refusal(from) : "the checks pass"),
  CoreRecordsRestoreRow: () => "the undo is recorded",
  CoreClaims: () => "the change is marked undone",
  CoreOpensSwapPlan: () => "the folder swap is noted in the journal",
  CoreWritesAtOnce: () => "the files are put back",
  CoreStartsWrite: () => "the write of the files starts",
  CoreEndsWrite: () => "the write of the files ends",
  CoreClosesPlan: () => "the swap is sealed",
  CoreWriteFails: () => "a file write fails",
  CoreReleasesAfterFailure: () => "a file write failed: the mark is cleared",
  CoreFinishesFailed: () => "the undo is recorded as failed",
  CoreFinishesDone: () => "the undo is recorded as finished",
  DesktopChecksClaim: (from, to) => (to.pc === "idle" ? refusal(from) : "the checks pass"),
  DesktopClaims: () => "the change is marked undone",
  DesktopChecksDrift: () => "the files are checked",
  DesktopRecordsRestoreRow: () => "the undo is recorded",
  DesktopWritesAtOnce: () => "the files are put back",
  DesktopRemovesLiveFiles: () => "the live files are removed",
  DesktopPutsOldFilesIn: () => "the old files are put in",
  DesktopWriteFails: () => "a file write fails",
  DesktopFinishesFailed: () => "the undo is recorded as failed",
  DesktopReleasesAfterFailure: (from) =>
    from.row[from.cur] === "failed"
      ? "a file write failed: the mark is cleared"
      : "Undo is refused: files changed since, it asks for Force",
  DesktopFinishesDone: () => "the undo is recorded as finished",
  WriterDies: () => "the app dies",
};

export const step = (action, from, to) => (labels[action] ? labels[action](from, to) : action);
