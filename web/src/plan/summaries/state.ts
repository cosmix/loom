import { useAtomValue, useStore } from "jotai/react";
import { atom, type Atom, type PrimitiveAtom } from "jotai/vanilla";
import { useCallback, useEffect, useRef } from "react";

import { apiGet, apiPost } from "@/plan/api/client";
import {
  summaryPanelSchema,
  type SummaryPanel,
  type SummaryQuestion,
} from "@/plan/api/schema-summaries";
import type { connectPlanSocket } from "@/plan/api/socket";
import { planSocketAtom } from "@/plan/state/atoms";
import type { Store } from "@/state/store";

export type PanelLoad =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; panel: SummaryPanel };

export const panelLoadAtom: PrimitiveAtom<PanelLoad> = atom<PanelLoad>({ status: "loading" });
export const summaryPanelAtom: Atom<SummaryPanel | null> = atom((get) => {
  const load = get(panelLoadAtom);
  return load.status === "ready" ? load.panel : null;
});
export const summaryQuestionAtom: Atom<SummaryQuestion | null> = atom(
  (get) => get(summaryPanelAtom)?.question ?? null,
);
export const turnCostsAtom: PrimitiveAtom<ReadonlyMap<string, number | null>> = atom<
  ReadonlyMap<string, number | null>
>(new Map());

export function turnCostKey(session: string, turn: string, id: string): string {
  return JSON.stringify([session, turn, id]);
}

// Counts socket pushes that write the panel. An HTTP response (seed GET, action POST) that started
// before a push is older than it, so it must not overwrite what the push set. Only deltas are
// compared, so one counter serves every store.
let panelPushes = 0;

/** Whether an HTTP panel that started at `pushesAtStart` is older than a push already shown. */
function supersededByPush(store: Store, pushesAtStart: number): boolean {
  return panelPushes !== pushesAtStart && store.get(panelLoadAtom).status === "ready";
}

function subscribeSummaryFeed(
  store: Store,
  socket: ReturnType<typeof connectPlanSocket> | null,
  reload: () => void,
): () => void {
  if (!socket) return () => {};
  const unsubscribes = [
    socket.subscribe("summary_panel", ({ type: _type, ...panel }) => {
      ++panelPushes;
      store.set(panelLoadAtom, { status: "ready", panel });
    }),
    socket.subscribe("summary_question", ({ type: _type, ...question }) => {
      ++panelPushes;
      store.set(panelLoadAtom, (load) =>
        load.status === "ready" ? { ...load, panel: { ...load.panel, question } } : load,
      );
    }),
    socket.subscribe("summary_question_closed", ({ id }) => {
      ++panelPushes;
      store.set(panelLoadAtom, (load) =>
        load.status === "ready" && load.panel.question?.id === id
          ? { ...load, panel: { ...load.panel, question: null } }
          : load,
      );
    }),
    socket.subscribe("summary_turn_cost", ({ session, turn, rows }) => {
      store.set(turnCostsAtom, (current) => {
        const costs = new Map(current);
        for (const row of rows) {
          costs.set(turnCostKey(session, turn, row.id), row.tokens ?? null);
        }
        return costs;
      });
    }),
    socket.onResync(reload),
  ];
  return () => unsubscribes.forEach((unsubscribe) => unsubscribe());
}

export function useSummaryFeed(): { load: PanelLoad; reload: () => void } {
  const store = useStore();
  const load = useAtomValue(panelLoadAtom);
  const socket = useAtomValue(planSocketAtom);
  const mounted = useRef(false);
  const request = useRef(0);
  const reload = useCallback(() => {
    if (!mounted.current) return;
    const currentRequest = ++request.current;
    store.set(panelLoadAtom, (current) =>
      current.status === "ready" ? current : { status: "loading" },
    );
    const pushesAtStart = panelPushes;
    void apiGet("/api/summaries/panel", summaryPanelSchema).then((result) => {
      if (!mounted.current || request.current !== currentRequest) return;
      if (supersededByPush(store, pushesAtStart)) return;
      store.set(
        panelLoadAtom,
        result.ok
          ? { status: "ready", panel: result.value }
          : { status: "error", message: result.message },
      );
    });
  }, [store]);

  useEffect(() => {
    mounted.current = true;
    const unsubscribe = subscribeSummaryFeed(store, socket, reload);
    reload();
    return () => {
      mounted.current = false;
      ++request.current;
      unsubscribe();
    };
  }, [store, socket, reload]);

  return { load, reload };
}

export type SummaryActions = {
  rebuild(ids: string[]): Promise<string | null>;
  answer(questionId: string, rebuild: boolean): Promise<string | null>;
  release(ids: string[]): Promise<string | null>;
};

async function postPanel(store: Store, path: string, body: unknown): Promise<string | null> {
  const pushesAtStart = panelPushes;
  const result = await apiPost(path, body, summaryPanelSchema, { contentType: "json" });
  if (!result.ok) return result.message;
  if (supersededByPush(store, pushesAtStart)) return null;
  store.set(panelLoadAtom, { status: "ready", panel: result.value });
  return null;
}

export function useSummaryActions(): SummaryActions {
  const store = useStore();
  return {
    rebuild: (ids) => postPanel(store, "/api/summaries/rebuild", { ids }),
    answer: (question, rebuild) => postPanel(store, "/api/summaries/answer", { question, rebuild }),
    release: (ids) => postPanel(store, "/api/summaries/release", { ids }),
  };
}

export function useTurnTokens(
  session: string,
  turn: string,
  id: string,
): number | null | undefined {
  return useAtomValue(turnCostsAtom).get(turnCostKey(session, turn, id));
}
