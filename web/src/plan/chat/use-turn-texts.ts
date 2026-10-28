import { useAtomValue } from "jotai";
import { useEffect, useState } from "react";

import type { Turn } from "@/plan/api/schema-chat";
import type { DiffTexts } from "@/plan/api/schema-files";
import { fetchTurnDiff } from "@/plan/files/api";
import { connectionEpochAtom } from "@/plan/state/atoms";

/** One changed document's turn-start and turn-end text, or why they could not be fetched. */
export type TurnRow = { path: string; texts?: DiffTexts; error?: string };

/**
 * Fetches the turn's before/after text for every changed document once. The footer passes the
 * result to Restore so each document is requested a single time per turn. Empty while the turn
 * is open or the fetch is in flight.
 */
export function useTurnTexts(turn: Turn, sessionId: string): TurnRow[] {
  const [rows, setRows] = useState<TurnRow[]>([]);
  const epoch = useAtomValue(connectionEpochAtom);
  const paths = JSON.stringify(turn.changed_files.map((file) => file.path));
  useEffect(() => {
    let active = true;
    setRows([]);
    if (!turn.open)
      void Promise.all(
        (JSON.parse(paths) as string[])
          .filter((path) => path.startsWith("doc/"))
          .map(async (path): Promise<TurnRow> => {
            const result = await fetchTurnDiff(`${sessionId}/${turn.id}`, path);
            return result.ok ? { path, texts: result.value } : { path, error: result.message };
          }),
      ).then((value) => {
        if (active) setRows(value);
      });
    return () => {
      active = false;
    };
  }, [paths, sessionId, turn.id, turn.open, epoch]);
  return rows;
}
