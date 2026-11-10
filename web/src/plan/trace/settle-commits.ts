import { useAtomValue } from "jotai/react";
import { useEffect, useRef } from "react";

import type { SettleCommitOutcome } from "@/plan/api/schema-trace";
import { planSocketAtom } from "@/plan/state/atoms";

/**
 * Calls `onOutcome` with every `settle_commit` the plan socket brings for the document at `path`
 * (a repository path), whichever browser's settle it ends. The latest `onOutcome` is the one
 * called, so a caller need not keep it stable.
 */
export function useSettleCommits(
  path: string,
  onOutcome: (outcome: SettleCommitOutcome) => void,
): void {
  const socket = useAtomValue(planSocketAtom);
  const latest = useRef(onOutcome);
  useEffect(() => {
    latest.current = onOutcome;
  });
  useEffect(() => {
    if (!socket) return;
    return socket.subscribe("settle_commit", ({ type: _type, ...outcome }) => {
      if (outcome.path === path) latest.current(outcome);
    });
  }, [socket, path]);
}
