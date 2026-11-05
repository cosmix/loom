import { createContext, useContext } from "react";

import { targetKey } from "./column-context";
import type { ColumnTarget } from "./types";

/**
 * For each source passage in the reading order, the roving key of its first showing. A passage
 * opened by two separate fragments shows its body at that first key only.
 */
export function firstPassageKeys(order: ColumnTarget[]): Map<string, string> {
  const first = new Map<string, string>();
  for (const target of order) {
    if (target.kind === "passage" && !first.has(target.id)) first.set(target.id, targetKey(target));
  }
  return first;
}

/** What a repeated passage needs to point back at its first showing. */
export type SamePassage = {
  first: Map<string, string>;
  goTo: (key: string) => void;
};

export const SamePassageContext = createContext<SamePassage | null>(null);

/**
 * The action that moves focus to the first showing of `anchor` (its `path#anchor` key), or null
 * where the passage at `ownKey` is itself the first showing, is unknown to the order, or no
 * column provides the context.
 */
export function useSamePassage(anchor: string, ownKey: string): (() => void) | null {
  const same = useContext(SamePassageContext);
  const first = same?.first.get(anchor);
  if (!same || first === undefined || first === ownKey) return null;
  return () => same.goTo(first);
}
