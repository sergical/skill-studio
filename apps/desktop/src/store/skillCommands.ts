// ============================================================================
// Skill Studio - Skill commands on the store
// The backend commands a component may call. Components reach the IPC layer
// (`lib/skill-api.ts`) only through the store, so this list is the whole
// surface between the two.
// ============================================================================

import {
  appVersion,
  checkForUpdate,
  getDiscoverySources,
  getEditorChoices,
  getTelemetryEnabled,
  getUpdateStatus,
  installUpdate,
  listProjectFolders,
  listSkillEvents,
  materializeHarnessRoot,
  materializeHarnessRootThenDisable,
  onUpdateStatus,
  openSkillPath,
  restoreSkillEvent,
  setDiscoverySource,
  setPreferredEditor,
  setTelemetryEnabled,
  skillLocalEdits,
} from "../lib/skill-api";

export const skillCommands = {
  appVersion,
  checkForUpdate,
  getDiscoverySources,
  getEditorChoices,
  getTelemetryEnabled,
  getUpdateStatus,
  installUpdate,
  listProjectFolders,
  listSkillEvents,
  materializeHarnessRoot,
  materializeHarnessRootThenDisable,
  onUpdateStatus,
  openSkillPath,
  restoreSkillEvent,
  setDiscoverySource,
  setPreferredEditor,
  setTelemetryEnabled,
  skillLocalEdits,
};

export type SkillCommands = typeof skillCommands;
