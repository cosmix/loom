import { useCallback, useEffect, useRef } from "react";

import { saveDraft, type SaveDraftResult } from "./drafts";

/** Typing writes a draft after this long a pause. */
export const DRAFT_DEBOUNCE_MS = 500;

type Job = { path: string; openedHash: string; text: string };

/**
 * Debounced draft writes. Every `saveDraft` re-parses all stored drafts and writes up to 1 MiB,
 * which is too much work per keystroke. The pending write is flushed when the page is hidden
 * or unloading, when the window loses focus and on unmount, so a pause never loses text.
 * `cancel` drops the pending write; call it before clearing drafts so a late flush cannot
 * bring one back.
 */
export function useDraftWriter(onResult: (result: SaveDraftResult) => void) {
  const job = useRef<Job | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const report = useRef(onResult);
  report.current = onResult;

  const cancel = useCallback(() => {
    clearTimeout(timer.current);
    job.current = null;
  }, []);

  const flush = useCallback(() => {
    clearTimeout(timer.current);
    const pending = job.current;
    job.current = null;
    if (pending) report.current(saveDraft(pending.path, pending.openedHash, pending.text));
  }, []);

  const schedule = useCallback(
    (path: string, openedHash: string, text: string) => {
      if (job.current && job.current.path !== path) flush();
      job.current = { path, openedHash, text };
      clearTimeout(timer.current);
      timer.current = setTimeout(flush, DRAFT_DEBOUNCE_MS);
    },
    [flush],
  );

  useEffect(() => {
    const onVisibility = () => {
      if (document.visibilityState === "hidden") flush();
    };
    window.addEventListener("beforeunload", flush);
    window.addEventListener("blur", flush);
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      window.removeEventListener("beforeunload", flush);
      window.removeEventListener("blur", flush);
      document.removeEventListener("visibilitychange", onVisibility);
      flush();
    };
  }, [flush]);

  return { schedule, cancel };
}
