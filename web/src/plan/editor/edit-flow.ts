import { useAtomValue, useStore } from "jotai/react";
import { useCallback, useEffect, useReducer, useRef, useState, type Dispatch } from "react";

import type { DocView } from "@/plan/api/schema-files";
import { fetchDoc, markChanged, saveDoc } from "@/plan/files/api";
import { docChangedAtom, treeAtom } from "@/plan/files/atoms";
import { renamedPlan } from "@/plan/files/tree";
import { treeDocs } from "@/plan/files/tree-docs";
import type { Store } from "@/state/store";

import { useDraftWriter } from "./draft-writer";
import { clearDrafts, findDrafts, type SaveDraftResult } from "./drafts";
import type { StageSpan } from "./editor-provenance";
import { pendingEditFor, requestEditAtom } from "./request-edit";
import { initialSaveFlow, saveFlow, type SaveFlowEvent, type SaveFlowState } from "./save-flow";

// `adopted` is the editor id of the stored draft this tab took over, so discard and a clean
// save remove that copy too; clearDrafts alone only removes this page load's own copies.
type Start = { flow: SaveFlowState; restored: boolean; adopted: string | null };

/**
 * The opening state: a requested edit (plan 9's Restore) or a stored draft. Text for
 * an older hash opens the merge at once and is never loaded as the editor text.
 */
function startState(doc: DocView, store: Store): Start {
  const base = initialSaveFlow(doc.text, doc.hash);
  const merge = (text: string): SaveFlowState =>
    saveFlow(base, { type: "merge", currentText: doc.text, currentHash: doc.hash, text });
  const request = pendingEditFor(store, doc.path);
  if (request) {
    const flow =
      request.openedHash === doc.hash
        ? saveFlow(base, { type: "change", text: request.text })
        : merge(request.text);
    return { flow, restored: false, adopted: null };
  }
  const drafts = findDrafts(doc.path);
  const same = drafts.find((draft) => draft.openedHash === doc.hash);
  if (same) {
    const flow = saveFlow(base, { type: "change", text: same.text });
    return { flow, restored: true, adopted: same.editorId };
  }
  const older = drafts[0];
  return {
    flow: older ? merge(older.text) : base,
    restored: false,
    adopted: older?.editorId ?? null,
  };
}

export function draftNote(result: SaveDraftResult | null): string | null {
  if (result === null || result === "saved") return null;
  const why =
    result === "unavailable" ? "this browser does not allow storage" : "browser storage is full";
  return `Unsaved text is not being kept: ${why}`;
}

/** Where the open file went, from the tree and the watcher's last word about it. */
export function useGone(doc: DocView): string | null {
  const tree = useAtomValue(treeAtom);
  const changes = useAtomValue(docChangedAtom);
  const known = Object.prototype.hasOwnProperty.call(changes, doc.path);
  if (known && changes[doc.path] === null) return "deleted";
  if (!tree || treeDocs(tree).some((entry) => entry.path === doc.path)) return null;
  const moved = doc.plan ? renamedPlan(tree, doc.plan.slug.toLowerCase(), doc.path) : null;
  return moved?.path ? `renamed to ${moved.path}` : "deleted";
}

export function stageSpans(doc: DocView): StageSpan[] {
  const total = doc.text.split("\n").length;
  const stages = (doc.plan?.stages ?? []).flatMap((stage) =>
    stage.line === null ? [] : [{ id: stage.id, line: stage.line }],
  );
  stages.sort((a, b) => a.line - b.line);
  return stages.map((stage, index) => ({
    id: stage.id,
    start_line: stage.line,
    end_line: (stages[index + 1]?.line ?? total + 1) - 1,
  }));
}

type Latest = { current: SaveFlowState };

function useFlowActions(
  path: string,
  store: Store,
  latest: Latest,
  dispatch: Dispatch<SaveFlowEvent>,
  canAuthor: boolean,
  cancelDraft: () => void,
  clearAll: () => void,
) {
  const author = useRef(canAuthor);
  author.current = canAuthor;
  const [compareError, setCompareError] = useState<string | null>(null);

  // Every save path (button, Ctrl-S in either editor, Save and close) comes through here.
  const save = useCallback(
    (withErrors: boolean) => {
      const { status, text, openedHash } = latest.current;
      if (!author.current || status === "saving") return;
      dispatch({ type: "save" });
      void saveDoc(path, text, openedHash, withErrors).then((result) => {
        dispatch({ type: "result", result });
        // Marks the open document stale so the reading view refetches without the watcher.
        if (result.ok && result.value.result === "saved")
          markChanged(store, path, result.value.hash);
      });
    },
    [path, store, latest, dispatch],
  );

  const compare = useCallback(async () => {
    const result = await fetchDoc(path);
    if (!result.ok) return setCompareError(result.message);
    setCompareError(null);
    dispatch({
      type: "merge",
      currentText: result.value.text,
      currentHash: result.value.hash,
      text: latest.current.text,
    });
  }, [path, latest, dispatch]);

  const discard = useCallback(() => {
    cancelDraft();
    clearAll();
    dispatch({ type: "discard" });
  }, [dispatch, cancelDraft, clearAll]);

  return { compareError, save, compare, discard };
}

export function useEditFlow(doc: DocView, canAuthor: boolean) {
  const store = useStore();
  const [start] = useState(() => startState(doc, store));
  const [state, dispatch] = useReducer(saveFlow, start.flow);
  const [restored, setRestored] = useState(start.restored);
  const [draftResult, setDraftResult] = useState<SaveDraftResult | null>(null);
  const latest = useRef(state);
  latest.current = state;
  const { path } = doc;
  // The adopted copy is removed ahead of this load's first draft write, so a later reload
  // cannot find it and bring back text discarded from a newer copy. Once removed (or cleared
  // by a discard or clean save) it is dropped: that editor id may now belong to a tab still
  // open, whose newer copy must not be deleted.
  const adopted = useRef(start.adopted);
  const clearAdopted = useCallback(() => {
    if (adopted.current) clearDrafts(path, adopted.current);
    adopted.current = null;
  }, [path]);
  const { schedule: scheduleDraft, cancel: cancelDraft } = useDraftWriter(
    setDraftResult,
    clearAdopted,
  );
  const clearAll = useCallback(() => {
    clearDrafts(path);
    clearAdopted();
  }, [path, clearAdopted]);
  const actions = useFlowActions(path, store, latest, dispatch, canAuthor, cancelDraft, clearAll);

  useEffect(() => {
    if (pendingEditFor(store, path)) store.set(requestEditAtom, null);
  }, [store, path]);

  useEffect(() => {
    if (!state.clearDrafts) return;
    cancelDraft();
    clearAll();
    setRestored(false);
    dispatch({ type: "draftsCleared" });
  }, [state.clearDrafts, cancelDraft, clearAll]);

  const change = useCallback(
    (text: string) => {
      dispatch({ type: "change", text });
      scheduleDraft(path, latest.current.openedHash, text);
      setRestored(false);
    },
    [path, scheduleDraft],
  );

  return { state, restored, draftResult, change, ...actions };
}

export type EditFlow = ReturnType<typeof useEditFlow>;
