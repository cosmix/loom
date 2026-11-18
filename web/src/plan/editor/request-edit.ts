import { atom } from "jotai/vanilla";

import { repoRel, toAddress } from "@/plan/files/address";
import type { Store } from "@/state/store";

type EditRequest = { path: string; text: string; openedHash: string };

export const requestEditAtom = atom<EditRequest | null>(null);

export function requestEdit(store: Store, req: EditRequest): string {
  store.set(requestEditAtom, req);
  return `${toAddress(req.path)}?edit=1`;
}

/**
 * The pending request for a document, whether it named the path repo-relative
 * (`doc/product/a.md`) or doc-relative (`product/a.md`, the form a save request uses).
 */
export function pendingEditFor(store: Store, path: string): EditRequest | null {
  const request = store.get(requestEditAtom);
  return request && repoRel(request.path) === repoRel(path) ? request : null;
}
