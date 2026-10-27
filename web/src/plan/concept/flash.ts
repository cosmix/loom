import { useEffect, useRef, useState } from "react";

import { useMediaQuery } from "@/lib/use-media-query";
import type { ConceptPage } from "@/plan/api/schema-concept";

import { pageSentences } from "./open-state";
import type { ConceptPageState } from "./page-state";

/** The animation lasts one second; the class is cleared a little after it ends. */
const FLASH_MS = 1200;
/** Without animation the wash is shown, then cleared after three seconds (section 05). */
const REDUCED_FLASH_MS = 3000;

export const FLASH_CLASS = "concept-flash";

/** `<page key>|<sentence id>`: sentence ids repeat across the summaries an overview cites. */
export function flashKey(pageKey: string, sentenceId: string): string {
  return `${pageKey}|${sentenceId}`;
}

/** Ids of `next` that the page `last` did not hold. */
export function newSentenceIds(last: ConceptPage, next: ConceptPage): string[] {
  const known = new Set(pageSentences(last).map((sentence) => sentence.id));
  return pageSentences(next)
    .map((sentence) => sentence.id)
    .filter((id) => !known.has(id));
}

export type KeyedState = { key: string; state: ConceptPageState };

/**
 * Sentences a page replacement brought, flashing for about one second (three seconds under
 * reduced motion). The pages seen when the hook first runs flash nothing, a mark-only update
 * (`previous === page`) flashes nothing, and unchanged ids never flash. `onArrival` receives
 * the number of new sentences of the primary page (the first state) for the live region.
 */
export function useSentenceFlash(
  states: KeyedState[],
  onArrival: (count: number) => void,
): ReadonlySet<string> {
  const reduced = useMediaQuery("(prefers-reduced-motion: reduce)");
  const seen = useRef(new Map<string, ConceptPage | null>());
  const [flashing, setFlashing] = useState<ReadonlySet<string>>(new Set());
  const latest = useRef({ onArrival, reduced });
  latest.current = { onArrival, reduced };
  const timers = useRef(new Set<ReturnType<typeof setTimeout>>());

  // Timers outlive effect re-runs (below) and are cleared only when the hook unmounts.
  useEffect(() => {
    const pending = timers.current;
    return () => pending.forEach(clearTimeout);
  }, []);

  useEffect(() => {
    const fresh: string[] = [];
    let primary = 0;
    states.forEach(({ key, state }, index) => {
      const last = seen.current.get(key);
      seen.current.set(key, state.page);
      if (!last || !state.page || last === state.page || state.previous === state.page) return;
      const ids = newSentenceIds(last, state.page);
      if (index === 0) primary = ids.length;
      fresh.push(...ids.map((id) => flashKey(key, id)));
    });
    if (primary > 0) latest.current.onArrival(primary);
    if (fresh.length === 0) return;
    setFlashing((current) => new Set([...current, ...fresh]));
    // Not cleared on re-run: a later render must not strand the wash on screen.
    const timer = setTimeout(
      () => {
        timers.current.delete(timer);
        setFlashing((current) => new Set([...current].filter((key) => !fresh.includes(key))));
      },
      latest.current.reduced ? REDUCED_FLASH_MS : FLASH_MS,
    );
    timers.current.add(timer);
  }, [states]);

  return flashing;
}

/**
 * True for about one second after `token` changes (three seconds under reduced motion); the
 * first value flashes nothing. A passage uses its document hash as the token.
 */
export function useFlashOnChange(token: string | null): boolean {
  const reduced = useMediaQuery("(prefers-reduced-motion: reduce)");
  const last = useRef(token);
  const [flashing, setFlashing] = useState(false);
  const timers = useRef(new Set<ReturnType<typeof setTimeout>>());

  useEffect(() => {
    if (last.current === token) return;
    last.current = token;
    if (token === null) return;
    setFlashing(true);
    const timer = setTimeout(() => setFlashing(false), reduced ? REDUCED_FLASH_MS : FLASH_MS);
    timers.current.add(timer);
  }, [token, reduced]);

  useEffect(() => {
    const pending = timers.current;
    return () => pending.forEach(clearTimeout);
  }, []);

  return flashing;
}
