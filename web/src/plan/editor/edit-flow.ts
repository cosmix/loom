import { useAtomValue, useStore } from "jotai/react";
import { useCallback, useEffect, useReducer, useRef, useState, type Dispatch } from "react";

import type { DocView } from "@/plan/api/schema-files";
import { fetchDoc, saveDoc } from "@/plan/files/api";
import { docChangedAtom, treeAtom } from "@/plan/files/atoms";
import { renamedPlan } from "@/plan/files/tree";
import { treeDocs } from "@/plan/files/tree-docs";
import type { Store } from "@/state/store";

import { useDraftWriter } from "./draft-writer";
import { clearDrafts, findDrafts, type SaveDraftResult } from "./drafts";
import type { StageSpan } from "./editor-provenance";
import { pendingEditFor, requestEditAtom } from "./request-edit";
import { initialSaveFlow, saveFlow, type SaveFlowEvent, type SaveFlowState } from "./save-flow";

type Start = { flow: SaveFlowState; restored: boolean };

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
    return { flow, restored: false };
  }
  const drafts = findDrafts(doc.path);
  const same = drafts.find((draft) => draft.openedHash === doc.hash);
  if (same) return { flow: saveFlow(base, { type: "change", text: same.text }), restored: true };
  const older = drafts[0];
  return { flow: older ? merge(older.text) : base, restored: false };
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
  latest: Latest,
  dispatch: Dispatch<SaveFlowEvent>,
  canAuthor: boolean,
  cancelDraft: () => void,
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
      void saveDoc(path, text, openedHash, withErrors).then((result) =>
        dispatch({ type: "result", result }),
      );
    },
    [path, latest, dispatch],
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
    clearDrafts(path);
    dispatch({ type: "discard" });
  }, [path, dispatch, cancelDraft]);

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
  const { schedule: scheduleDraft, cancel: cancelDraft } = useDraftWriter(setDraftResult);
  const actions = useFlowActions(path, latest, dispatch, canAuthor, cancelDraft);

  useEffect(() => {
    if (pendingEditFor(store, path)) store.set(requestEditAtom, null);
  }, [store, path]);

  useEffect(() => {
    if (!state.clearDrafts) return;
    cancelDraft();
    clearDrafts(path);
    setRestored(false);
    dispatch({ type: "draftsCleared" });
  }, [state.clearDrafts, path, cancelDraft]);

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
