import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
  type FocusEvent,
  type MutableRefObject,
  type RefObject,
} from "react";

import { targetKey } from "./column-context";
import { survivorKey } from "./column-model";
import type { ColumnTarget, OpenRun } from "./types";

export function focusElement(
  root: HTMLElement | null,
  key: string | null,
): HTMLElement | undefined {
  if (!root || key === null) return undefined;
  return Array.from(root.querySelectorAll<HTMLElement>("[data-focus]")).find(
    (element) => element.getAttribute("data-focus") === key,
  );
}

export type Roving = {
  /** The key of the roving target: the focused one, else the first. */
  current: string | null;
  /** Whether DOM focus is inside the column (its own tab stop then steps aside). */
  inside: boolean;
  insideRef: MutableRefObject<boolean>;
  move: (key: string) => void;
  onFocus: (event: FocusEvent<HTMLElement>) => void;
  onBlur: (event: FocusEvent<HTMLElement>) => void;
};

/**
 * A page replacement and the flag the open-state carry sets when it writes (see
 * `useScrollAnchor`): the write re-renders the column with the carried runs before paint.
 */
export type Carry = { replacement: unknown; carried: MutableRefObject<boolean> };

type Follow = {
  root: RefObject<HTMLElement | null>;
  order: ColumnTarget[];
  open: OpenRun[];
  carry: Carry;
  current: string | null;
  /** The focus key when its target left the order, else null. */
  lostKey: string | null;
  insideRef: MutableRefObject<boolean>;
  want: MutableRefObject<boolean>;
  setFocusKey: (key: string | null) => void;
};

/**
 * After every commit: a focus target that left the order is replaced by its survivor, and
 * DOM focus follows the roving target when a move asked for it or focus fell out of the column.
 * A replacement's first commit still shows the runs open before it; when the carry wrote, the
 * survivor waits for the carry's commit and is judged against the order before the replacement,
 * so a run the carry reopens under a new sentence id keeps the focus inside it.
 */
function useFocusFollow(follow: Follow): void {
  const { root, order, open, carry, current, lostKey, insideRef, want, setFocusKey } = follow;
  const prev = useRef({ order, open });
  const seen = useRef(carry.replacement);
  useLayoutEffect(() => {
    const replaced = seen.current !== carry.replacement;
    seen.current = carry.replacement;
    if (replaced && carry.carried.current) return;
    if (lostKey !== null) {
      setFocusKey(survivorKey(prev.current, { order, open }, lostKey));
      want.current = insideRef.current;
    }
    prev.current = { order, open };
  });

  useLayoutEffect(() => {
    // A target that left the order is replaced first; focusing the fallback would overwrite it.
    if (lostKey !== null) return;
    const element = focusElement(root.current, current);
    const lost = insideRef.current && !root.current?.contains(document.activeElement);
    if ((want.current || lost) && element) {
      element.focus();
      want.current = false;
    }
  });
}

/**
 * The roving focus over the reading order. Focus is a real DOM focus on the target's element,
 * so a screen reader hears the sentence and its `aria-expanded`. When the target leaves the
 * order (Fold all, a rebuild, a removed passage) it moves to its nearest surviving ancestor,
 * and DOM focus follows if it was in the column, so it never falls to `body`.
 */
export function useRovingFocus(
  root: RefObject<HTMLElement | null>,
  order: ColumnTarget[],
  open: OpenRun[],
  carry: Carry,
): Roving {
  const [focusKey, setFocusKey] = useState<string | null>(null);
  const [, bump] = useReducer((n: number) => n + 1, 0);
  const [inside, setInside] = useState(false);
  const insideRef = useRef(false);
  const want = useRef(false);
  const keys = useMemo(() => new Set(order.map(targetKey)), [order]);
  const valid = focusKey !== null && keys.has(focusKey);
  const first = order[0] ? targetKey(order[0]) : null;
  const current = valid ? focusKey : first;
  const lostKey = focusKey !== null && !valid ? focusKey : null;
  useFocusFollow({ root, order, open, carry, current, lostKey, insideRef, want, setFocusKey });

  const move = useCallback((key: string) => {
    want.current = true;
    setFocusKey(key);
    // A caller writes the store in the same handler, so the target's element may be about to
    // change (a sentence becomes its run group, or the reverse): focus is taken by the layout
    // effect after that commit, never here. The bump forces the commit when the key is unchanged.
    bump();
  }, []);
  const onFocus = (event: FocusEvent<HTMLElement>) => {
    insideRef.current = true;
    setInside(true);
    if (event.target === event.currentTarget) {
      focusElement(event.currentTarget, current)?.focus();
      return;
    }
    const key = event.target.getAttribute("data-focus");
    if (key !== null) setFocusKey(key);
  };
  const onBlur = (event: FocusEvent<HTMLElement>) => {
    const next = event.relatedTarget as Node | null;
    // A removed element can blur without a new target; only a real move away leaves the column.
    if (next && event.currentTarget.contains(next)) return;
    if (!event.target.isConnected) return;
    insideRef.current = false;
    setInside(false);
  };
  return { current, inside, insideRef, move, onFocus, onBlur };
}

/** The nearest ancestor that scrolls, else the window. */
function scrollParent(element: HTMLElement): HTMLElement | null {
  for (let node = element.parentElement; node; node = node.parentElement) {
    if (/(auto|scroll)/.test(getComputedStyle(node).overflowY)) return node;
  }
  return null;
}

export function scrollByY(element: HTMLElement, delta: number): void {
  const parent = scrollParent(element);
  if (parent) parent.scrollBy(0, delta);
  else window.scrollBy(0, delta);
}

type Snapshot = { key: string; top: number };

function firstVisible(root: HTMLElement): HTMLElement | undefined {
  return Array.from(root.querySelectorAll<HTMLElement>("[data-sentence-id]")).find(
    (element) => element.getBoundingClientRect().bottom > 0,
  );
}

/**
 * Scroll anchoring (section 05, "Rebuilds while reading"): the column remembers where the
 * focused sentence (else the first visible one) is on screen after every commit and scroll,
 * and after a page replacement scrolls by whatever it moved, so content that changes above
 * the viewport never moves the text the reader is looking at.
 *
 * A replacement takes two commits: the new page renders with the old open runs, then the
 * carry's store write (a layout effect, so React re-renders synchronously before paint)
 * renders the carried runs. `carried` is set by that write; the commit after it compensates
 * too, against the anchor recorded at the end of the first.
 */
export function useScrollAnchor(
  root: RefObject<HTMLElement | null>,
  roving: Roving,
  replacement: unknown,
  carried: MutableRefObject<boolean>,
): void {
  const snapshot = useRef<Snapshot | null>(null);
  const seen = useRef(replacement);
  const { current, insideRef } = roving;
  const record = useCallback(() => {
    const column = root.current;
    if (!column) return;
    const focused = insideRef.current ? focusElement(column, current) : undefined;
    const anchor = focused ?? firstVisible(column);
    const key = anchor?.getAttribute("data-focus");
    snapshot.current = anchor && key ? { key, top: anchor.getBoundingClientRect().top } : null;
  }, [root, current, insideRef]);

  useLayoutEffect(() => {
    const replaced = seen.current !== replacement;
    seen.current = replacement;
    if (!replaced) {
      if (!carried.current) return;
      carried.current = false;
    }
    const before = snapshot.current;
    const element = before ? focusElement(root.current, before.key) : undefined;
    if (!before || !element) return;
    const delta = element.getBoundingClientRect().top - before.top;
    if (Math.abs(delta) > 0.5) scrollByY(element, delta);
  });
  useLayoutEffect(record);
  useEffect(() => {
    document.addEventListener("scroll", record, { capture: true, passive: true });
    return () => document.removeEventListener("scroll", record, { capture: true });
  }, [record]);
}

/**
 * Opening scrolls only when the opened point would leave the screen: the top of the opened
 * content, below the viewport's last line or above its top. Otherwise nothing moves.
 */
export function useOpenScroll(
  root: RefObject<HTMLElement | null>,
  pending: MutableRefObject<string | null>,
  open: OpenRun[],
): void {
  useLayoutEffect(() => {
    const key = pending.current;
    pending.current = null;
    const element = focusElement(root.current, key);
    if (!element) return;
    const top = element.getBoundingClientRect().top;
    if (top < 0 || top > window.innerHeight - 24) scrollByY(element, top - window.innerHeight / 3);
  }, [root, pending, open]);
}
