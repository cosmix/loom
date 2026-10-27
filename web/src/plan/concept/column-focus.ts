import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type FocusEvent,
  type MutableRefObject,
  type RefObject,
} from "react";

import { targetKey } from "./column-context";
import { survivorKey } from "./column-model";
import type { FocusTarget, OpenRun } from "./types";

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
 * The roving focus over the reading order. Focus is a real DOM focus on the target's element,
 * so a screen reader hears the sentence and its `aria-expanded`. When the target leaves the
 * order (Fold all, a rebuild, a removed passage) it moves to its nearest surviving ancestor,
 * and DOM focus follows if it was in the column, so it never falls to `body`.
 */
export function useRovingFocus(
  root: RefObject<HTMLElement | null>,
  order: FocusTarget[],
  open: OpenRun[],
): Roving {
  const [focusKey, setFocusKey] = useState<string | null>(null);
  const [inside, setInside] = useState(false);
  const insideRef = useRef(false);
  const want = useRef(false);
  const prev = useRef({ order, open });
  const keys = useMemo(() => new Set(order.map(targetKey)), [order]);
  const valid = focusKey !== null && keys.has(focusKey);
  const first = order[0] ? targetKey(order[0]) : null;
  const current = valid ? focusKey : first;

  useLayoutEffect(() => {
    if (focusKey !== null && !valid) {
      setFocusKey(survivorKey(prev.current, { order, open }, focusKey));
      want.current = insideRef.current;
    }
    prev.current = { order, open };
  });

  useLayoutEffect(() => {
    // A target that left the order is replaced first; focusing the fallback would overwrite it.
    if (focusKey !== null && !valid) return;
    const element = focusElement(root.current, current);
    const lost = insideRef.current && !root.current?.contains(document.activeElement);
    if ((want.current || lost) && element) {
      element.focus();
      want.current = false;
    }
  });

  const move = useCallback((key: string) => {
    want.current = true;
    setFocusKey(key);
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
 */
export function useScrollAnchor(
  root: RefObject<HTMLElement | null>,
  roving: Roving,
  replacement: unknown,
): void {
  const snapshot = useRef<Snapshot | null>(null);
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
    const before = snapshot.current;
    const element = before ? focusElement(root.current, before.key) : undefined;
    if (!before || !element) return;
    const delta = element.getBoundingClientRect().top - before.top;
    if (Math.abs(delta) > 0.5) scrollByY(element, delta);
  }, [root, replacement]);
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
