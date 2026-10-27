import { atom } from "jotai/vanilla";

import type { DocView, FileTree, ValidationView } from "@/plan/api/schema-files";

export const treeAtom = atom<FileTree | null>(null);
export const openDocAtom = atom<{ doc: DocView; seenHash: string } | null>(null);
export const docChangedAtom = atom<Record<string, string | null>>({});
export const validationAtom = atom<Record<string, ValidationView>>({});
export const previousOpenTextAtom = atom<{ path: string; text: string } | null>(null);

export const openDocIsStaleAtom = atom((get) => {
  const open = get(openDocAtom);
  if (!open) return false;
  const changes = get(docChangedAtom);
  return (
    Object.prototype.hasOwnProperty.call(changes, open.doc.path) &&
    changes[open.doc.path] !== open.seenHash
  );
});
