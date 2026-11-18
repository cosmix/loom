import { useEffect } from "react";
import { apiGet, apiPost, type ApiResult } from "@/plan/api/client";
import {
  diffTextsSchema,
  docViewSchema,
  fileTreeSchema,
  saveResultSchema,
  searchResultsSchema,
  type DiffTexts,
  type DocView,
  type FileTree,
  type SaveResult,
  type SearchResults,
} from "@/plan/api/schema-files";
import type { connectPlanSocket } from "@/plan/api/socket";
import { docRel } from "@/plan/files/address";
import { docChangedAtom, openDocAtom, treeAtom, validationAtom } from "@/plan/files/atoms";
import { planSocketAtom } from "@/plan/state/atoms";
import type { Store } from "@/state/store";

/**
 * Callers pass repo-relative paths (`doc/...`, the form every response carries); the
 * server reads a request's `path` relative to `doc/`, so the conversion happens here.
 */
const docParam = (path: string): string => encodeURIComponent(docRel(path));

export const fetchTree = (): Promise<ApiResult<FileTree>> => apiGet("/api/tree", fileTreeSchema);
export const fetchDoc = (path: string): Promise<ApiResult<DocView>> =>
  apiGet(`/api/doc?path=${docParam(path)}`, docViewSchema);
export const searchDocs = (q: string, scope: string): Promise<ApiResult<SearchResults>> =>
  apiGet(
    `/api/search?q=${encodeURIComponent(q)}&scope=${encodeURIComponent(scope)}`,
    searchResultsSchema,
  );
export const fetchHeadDiff = (path: string): Promise<ApiResult<DiffTexts>> =>
  apiGet(`/api/diff/head?path=${docParam(path)}`, diffTextsSchema);
export const fetchTurnDiff = (turn: string, path: string): Promise<ApiResult<DiffTexts>> =>
  apiGet(`/api/diff/turn?turn=${encodeURIComponent(turn)}&path=${docParam(path)}`, diffTextsSchema);

/** A conflict (409) and a refused save (422) answer with a `SaveResult`, not an error. */
export function saveDoc(
  path: string,
  text: string,
  openedHash: string,
  withErrors: boolean,
): Promise<ApiResult<SaveResult>> {
  const query = `path=${docParam(path)}&hash=${encodeURIComponent(openedHash)}`;
  const flag = withErrors ? "&with_errors=1" : "";
  return apiPost(`/api/save?${query}${flag}`, text, saveResultSchema, {
    contentType: "text",
    resultStatuses: [409, 422],
  });
}

type PlanSocket = ReturnType<typeof connectPlanSocket>;

/** Bumped by every push and every refetch start, so an older response never overwrites a newer push. */
type Versions = { tree: number; doc: number; validation: number };

/** `null` marks the document gone: the open one refetches and finds it missing. */
const markChanged = (store: Store, path: string, hash: string | null): void =>
  store.set(docChangedAtom, (current) => ({ ...current, [path]: hash }));

function subscribeChanges(socket: PlanSocket, store: Store, versions: Versions): (() => void)[] {
  return [
    socket.subscribe("tree_changed", ({ type: _type, ...tree }) => {
      versions.tree += 1;
      store.set(treeAtom, tree);
    }),
    socket.subscribe("doc_changed", ({ path, hash }) => {
      versions.doc += 1;
      markChanged(store, path, hash);
    }),
    socket.subscribe("validation", ({ path, validation }) => {
      versions.validation += 1;
      store.set(validationAtom, (current) => ({ ...current, [path]: validation }));
    }),
  ];
}

function refreshTree(store: Store, versions: Versions, live: () => boolean): void {
  const started = ++versions.tree;
  void fetchTree().then((result) => {
    if (live() && versions.tree === started && result.ok) store.set(treeAtom, result.value);
  });
}

function refreshOpenDoc(store: Store, versions: Versions, live: () => boolean): void {
  const open = store.get(openDocAtom);
  if (!open) return;
  const path = open.doc.path;
  const docStarted = ++versions.doc;
  const validationStarted = ++versions.validation;
  void fetchDoc(path).then((result) => {
    if (!live() || store.get(openDocAtom)?.doc.path !== path) return;
    if (!result.ok) {
      if (result.status === 404 && versions.doc === docStarted) markChanged(store, path, null);
      return;
    }
    if (versions.doc === docStarted) markChanged(store, path, result.value.hash);
    const validation = result.value.plan?.validation;
    if (validation && versions.validation === validationStarted) {
      store.set(validationAtom, (current) => ({ ...current, [path]: validation }));
    }
  });
}

/**
 * Keeps the tree, change marks and validation current while file mode is mounted. The
 * socket delivers changes only while this runs, so each mount starts from a fresh tree
 * and no remembered changes, and a reconnect refetches the tree and the open document.
 */
export function useFileSocket(store: Store): void {
  useEffect(() => {
    let active = true;
    const live = () => active;
    const versions: Versions = { tree: 0, doc: 0, validation: 0 };
    store.set(docChangedAtom, {});
    store.set(validationAtom, {});
    refreshTree(store, versions, live);
    const socket = store.get(planSocketAtom);
    const cleanups = socket
      ? [
          ...subscribeChanges(socket, store, versions),
          socket.onResync(() => {
            refreshTree(store, versions, live);
            refreshOpenDoc(store, versions, live);
          }),
        ]
      : [];
    return () => {
      active = false;
      cleanups.forEach((cleanup) => cleanup());
    };
  }, [store]);
}
