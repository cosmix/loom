import { useAtomValue } from "jotai";
import type { JSX } from "react";

import type { Turn } from "@/plan/api/schema-chat";
import { useTurnTokens } from "@/plan/summaries/state";
import { SummaryTokens } from "@/plan/summaries/summary-tokens";
import { snapshotAtom } from "./model";

export function RebuiltSummaryExtras({ id, turn }: { id: string; turn: Turn }): JSX.Element | null {
  const snapshot = useAtomValue(snapshotAtom);
  // Turn ids repeat across sessions; the transcript retains the original turn objects.
  const session = snapshot.previous?.turns.includes(turn)
    ? snapshot.previous.session
    : snapshot.session;
  const tokens = useTurnTokens(session?.id ?? "", turn.id, id);
  return tokens === undefined ? null : <SummaryTokens tokens={tokens} />;
}
