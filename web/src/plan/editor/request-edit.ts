import { atom } from "jotai/vanilla";

import { toAddress } from "@/plan/files/address";
import type { Store } from "@/state/store";

type EditRequest = { path: string; text: string; openedHash: string };

export const requestEditAtom = atom<EditRequest | null>(null);

export function requestEdit(store: Store, req: EditRequest): string {
  store.set(requestEditAtom, req);
  return `${toAddress(req.path)}?edit=1`;
}
