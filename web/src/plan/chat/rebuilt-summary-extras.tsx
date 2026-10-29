import { useAtomValue } from "jotai";
import { selectAtom } from "jotai/vanilla/utils";
import { useMemo, type JSX } from "react";

import type { Turn } from "@/plan/api/schema-chat";
import { useTurnTokens } from "@/plan/summaries/state";
import { SummaryTokens } from "@/plan/summaries/summary-tokens";
import { snapshotAtom } from "./model";

/** The id of the chat session that holds `turn`; a string, so a row re-renders only when it changes. */
function useTurnSessionId(turn: Turn): string {
  const sessionIdAtom = useMemo(
    () =>
      selectAtom(snapshotAtom, (snapshot) => {
        // Turn ids repeat across sessions; the transcript retains the original turn objects.
        const session = snapshot.previous?.turns.includes(turn)
          ? snapshot.previous.session
          : snapshot.session;
        return session?.id ?? "";
      }),
    [turn],
  );
  return useAtomValue(sessionIdAtom);
}

export function RebuiltSummaryExtras({ id, turn }: { id: string; turn: Turn }): JSX.Element | null {
  const tokens = useTurnTokens(useTurnSessionId(turn), turn.id, id);
  return tokens === undefined ? null : <SummaryTokens tokens={tokens} />;
}
