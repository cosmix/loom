import { storageKey } from "@/plan/chat/storage";
import { serverInfoAtom } from "@/plan/state/atoms";
import { store } from "@/state/store";

import type { EditorMode } from "./codemirror";

const NAME = "editor-mode";

/** The key of the remembered mode, or null before the server info (and its project key) arrives. */
function modeKey(): string | null {
  const project = store.get(serverInfoAtom)?.project_key;
  return project ? storageKey(project, NAME) : null;
}

/**
 * The mode the editor opens in: the one remembered for this project, else live preview.
 * Blocked or failing storage, no project key and an unknown stored value all read as preview.
 */
export function readEditorMode(): EditorMode {
  try {
    const key = modeKey();
    return key !== null && localStorage.getItem(key) === "source" ? "source" : "preview";
  } catch {
    return "preview";
  }
}

/** Remembers the mode for this project. Writes nothing without a project key or storage. */
export function writeEditorMode(mode: EditorMode): void {
  try {
    const key = modeKey();
    if (key !== null) localStorage.setItem(key, mode);
  } catch {
    // The toolbar holds the mode for this session; only the memory across sessions is lost.
  }
}
