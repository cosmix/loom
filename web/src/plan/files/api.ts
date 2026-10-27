import { useEffect } from "react";
import { z } from "@/api/zod";
import { apiGet, type ApiResult } from "@/plan/api/client";
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
import { docChangedAtom, openDocAtom, treeAtom, validationAtom } from "@/plan/files/atoms";
import { planSocketAtom, unauthorizedAtom } from "@/plan/state/atoms";
import { store as defaultStore, type Store } from "@/state/store";

export const fetchTree = (): Promise<ApiResult<FileTree>> => apiGet("/api/tree", fileTreeSchema);
export const fetchDoc = (path: string): Promise<ApiResult<DocView>> =>
  apiGet(`/api/doc?path=${encodeURIComponent(path)}`, docViewSchema);
export const searchDocs = (q: string, scope: string): Promise<ApiResult<SearchResults>> =>
  apiGet(
    `/api/search?q=${encodeURIComponent(q)}&scope=${encodeURIComponent(scope)}`,
    searchResultsSchema,
  );
export const fetchHeadDiff = (path: string): Promise<ApiResult<DiffTexts>> =>
  apiGet(`/api/diff/head?path=${encodeURIComponent(path)}`, diffTextsSchema);
export const fetchTurnDiff = (turn: string, path: string): Promise<ApiResult<DiffTexts>> =>
  apiGet(
    `/api/diff/turn?turn=${encodeURIComponent(turn)}&path=${encodeURIComponent(path)}`,
    diffTextsSchema,
  );

const errorSchema = z.object({ error: z.string(), message: z.string().optional() });

export async function saveDoc(
  path: string,
  text: string,
  openedHash: string,
  withErrors: boolean,
): Promise<ApiResult<SaveResult>> {
  const query = `path=${encodeURIComponent(path)}&hash=${encodeURIComponent(openedHash)}`;
  let response: Response;
  try {
    response = await fetch(`/api/save?${query}${withErrors ? "&with_errors=1" : ""}`, {
      method: "POST",
      credentials: "same-origin",
      headers: { "Content-Type": "text/plain; charset=utf-8" },
      body: text,
    });
  } catch (error) {
    return {
      ok: false,
      status: 0,
      error: "unreachable",
      message: error instanceof Error ? error.message : String(error),
    };
  }
  if (response.status === 401) defaultStore.set(unauthorizedAtom, true);
  let body: unknown;
  try {
    body = await response.json();
  } catch {
    body = undefined;
  }
  if (response.ok || response.status === 409 || response.status === 422) {
    const parsed = saveResultSchema.safeParse(body);
    return parsed.success
      ? { ok: true, value: parsed.data }
      : { ok: false, status: 0, error: "bad_response", message: "Invalid server response" };
  }
  const parsed = errorSchema.safeParse(body);
  return {
    ok: false,
    status: response.status,
    error: parsed.success ? parsed.data.error : "http_error",
    message: parsed.success
      ? (parsed.data.message ?? parsed.data.error)
      : `HTTP ${response.status}`,
  };
}

export function useFileSocket(store: Store): void {
  useEffect(() => {
    const socket = store.get(planSocketAtom);
    if (!socket) return;
    let active = true;
    let treeVersion = 0;
    let docVersion = 0;
    let validationVersion = 0;
    const cleanups = [
      socket.subscribe("tree_changed", ({ type: _type, ...tree }) => {
        treeVersion += 1;
        store.set(treeAtom, tree);
      }),
      socket.subscribe("doc_changed", ({ path, hash }) => {
        docVersion += 1;
        store.set(docChangedAtom, (current) => ({ ...current, [path]: hash }));
      }),
      socket.subscribe("validation", ({ path, validation }) => {
        validationVersion += 1;
        store.set(validationAtom, (current) => ({ ...current, [path]: validation }));
      }),
      socket.onResync(() => {
        const treeAtStart = ++treeVersion;
        const docAtStart = ++docVersion;
        const validationAtStart = ++validationVersion;
        const open = store.get(openDocAtom);
        void fetchTree().then((result) => {
          if (active && treeVersion === treeAtStart && result.ok) store.set(treeAtom, result.value);
        });
        if (!open) return;
        void fetchDoc(open.doc.path).then((result) => {
          if (!active || !result.ok || store.get(openDocAtom)?.doc.path !== open.doc.path) return;
          if (docVersion === docAtStart) {
            store.set(docChangedAtom, (current) => ({
              ...current,
              [open.doc.path]: result.value.hash,
            }));
          }
          const validation = result.value.plan?.validation;
          if (validation && validationVersion === validationAtStart) {
            store.set(validationAtom, (current) => ({ ...current, [open.doc.path]: validation }));
          }
        });
      }),
    ];
    return () => {
      active = false;
      cleanups.forEach((cleanup) => cleanup());
    };
  }, [store]);
}
