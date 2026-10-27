import { useAtomValue, useStore } from "jotai/react";
import { loadable } from "jotai/utils";
import { useEffect, useState } from "react";

import type { SourceAnchor } from "@/plan/api/schema-concept";
import type { DocView } from "@/plan/api/schema-files";
import type { Store } from "@/state/store";

import { docViewAtom, passageOf } from "./passages";

const loadables = new Map<string, ReturnType<typeof makeLoadable>>();

function makeLoadable(path: string) {
  return loadable(docViewAtom(path));
}

/** One loadable per document, so every passage of a file shares a single fetch and atom. */
function docLoadable(path: string) {
  let atom = loadables.get(path);
  if (!atom) {
    atom = makeLoadable(path);
    loadables.set(path, atom);
  }
  return atom;
}

/**
 * The documents the open passages last rendered, per store, by repo path, and how many
 * mounted passages show each path. An entry goes when its last passage unmounts or its
 * refetch fails (the file was deleted or renamed, or could not be read).
 */
type Shown = { docs: Map<string, DocView>; users: Map<string, number> };
const shown = new WeakMap<Store, Shown>();

function shownOf(store: Store): Shown {
  let entry = shown.get(store);
  if (!entry) {
    entry = { docs: new Map(), users: new Map() };
    shown.set(store, entry);
  }
  return entry;
}

/** Counts a mounted passage of `path`; the path's document is dropped with its last passage. */
function useShownUser(store: Store, path: string): void {
  useEffect(() => {
    const entry = shownOf(store);
    entry.users.set(path, (entry.users.get(path) ?? 0) + 1);
    return () => {
      const left = (entry.users.get(path) ?? 1) - 1;
      if (left > 0) {
        entry.users.set(path, left);
        return;
      }
      entry.users.delete(path);
      entry.docs.delete(path);
    };
  }, [store, path]);
}

/** The last loaded document, kept while a refetch is in flight so the block never blanks. */
export function useKeptDoc(path: string): { doc: DocView | null; failed: boolean } {
  const store = useStore();
  const state = useAtomValue(docLoadable(path));
  const [kept, setKept] = useState<DocView | null>(null);
  const data = state.state === "hasData" ? state.data : null;
  const failed = state.state === "hasError";
  useShownUser(store, path);
  useEffect(() => {
    const { docs } = shownOf(store);
    if (failed) docs.delete(path);
    if (!data) return;
    setKept(data);
    docs.set(path, data);
  }, [store, path, data, failed]);
  // A failed refetch drops the kept document: the file is gone or unreadable, so the block
  // shows its notice instead of a section that may no longer exist.
  return { doc: failed ? null : (data ?? kept), failed };
}

/**
 * Whether a source section is still in its file, judged on the document a mounted passage of
 * that file last rendered. Undefined when no passage of the file is mounted or its last fetch
 * failed, so nothing is fetched and a dropped section reads "was removed". The runs whose
 * sections a rebuild drops show those passages, and a source change refetches them before the
 * rebuild arrives, so a kept document is current; a deleted file's refetch fails and drops it.
 */
export function sectionInShownDoc(store: Store, anchor: SourceAnchor): boolean | undefined {
  const doc = shown.get(store)?.docs.get(anchor.path);
  return doc ? passageOf(doc, anchor.anchor) !== null : undefined;
}
