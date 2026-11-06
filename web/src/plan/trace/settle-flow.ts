import { useStore } from "jotai/react";
import { useCallback, useState } from "react";

import { z } from "@/api/zod";
import { apiPost, type ApiResult } from "@/plan/api/client";
import { saveResultSchema } from "@/plan/api/schema-files";
import { unresolvedSchema } from "@/plan/api/schema-trace";
import { refreshDocView } from "@/plan/concept/passages";
import { docRel } from "@/plan/files/address";
import { docChangedAtom } from "@/plan/files/atoms";
import type { Store } from "@/state/store";

import { loadTrace } from "./state";

/** The route's other refusals that carry a body in 409 or 422 (`stale` while the trace trails). */
const refusalSchema = z.strictObject({ error: z.string(), message: z.string() });

/**
 * `POST /api/settle` answers a save result, the 409 `unresolved` answer without `confirm=1`, or a
 * refusal whose message is shown as it is.
 */
const settleAnswerSchema = z.union([saveResultSchema, unresolvedSchema, refusalSchema]);
type SettleAnswer = z.infer<typeof settleAnswerSchema>;

/** Where the control stands: at rest, sending, asking to confirm open points, or telling why not. */
export type SettlePhase =
  | { kind: "idle" }
  | { kind: "working" }
  | { kind: "confirm"; myCalls: number; openQuestions: number }
  | { kind: "notice"; message: string };

const CONFLICT_NOTICE =
  "This document changed while you were reading it. It has been reloaded; settle it again if it still reads right.";
const INVALID_NOTICE = "This plan has validation errors. Fix them before settling it.";

/** `path` is the repository path (`doc/...`); the route reads it relative to `doc/`. */
function postSettle(
  path: string,
  hash: string,
  flags: { clear: boolean; confirm: boolean },
): Promise<ApiResult<SettleAnswer>> {
  const query = new URLSearchParams({ path: docRel(path), hash });
  if (flags.clear) query.set("clear", "1");
  if (flags.confirm) query.set("confirm", "1");
  // A 409 (stale hash, unresolved points) and a 422 (plan errors) carry a typed answer.
  return apiPost(`/api/settle?${query}`, "", settleAnswerSchema, {
    contentType: "text",
    resultStatuses: [409, 422],
  });
}

/** The phase an answer leaves the control in, and the hash the document now has (if it moved). */
function outcome(result: ApiResult<SettleAnswer>): { phase: SettlePhase; moved: string | null } {
  if (!result.ok) return { phase: { kind: "notice", message: result.message }, moved: null };
  const answer = result.value;
  if ("error" in answer) {
    if (!("my_calls" in answer))
      return { phase: { kind: "notice", message: answer.message }, moved: null };
    const { my_calls: myCalls, open_questions: openQuestions } = answer;
    return { phase: { kind: "confirm", myCalls, openQuestions }, moved: null };
  }
  if (answer.result === "saved") return { phase: { kind: "idle" }, moved: answer.hash };
  if (answer.result === "conflict") {
    return { phase: { kind: "notice", message: CONFLICT_NOTICE }, moved: answer.current_hash };
  }
  return { phase: { kind: "notice", message: INVALID_NOTICE }, moved: null };
}

/**
 * Reloads the document the way a save does (its summary page's text and the open file's hash),
 * and the trace, which also arrives by `trace_changed` once the watcher sees the write.
 */
function reload(store: Store, feature: string, path: string, hash: string | null): void {
  refreshDocView(store, path);
  if (hash !== null) store.set(docChangedAtom, (current) => ({ ...current, [path]: hash }));
  void loadTrace(store, feature);
}

export function useSettle(feature: string, path: string, hash: string | null) {
  const store = useStore();
  const [phase, setPhase] = useState<SettlePhase>({ kind: "idle" });

  const send = useCallback(
    /** Resolves true when the line was written, so the caller can move focus off the button. */
    async (flags: { clear: boolean; confirm: boolean }): Promise<boolean> => {
      if (hash === null) return false;
      setPhase({ kind: "working" });
      const next = outcome(await postSettle(path, hash, flags));
      if (next.moved !== null) reload(store, feature, path, next.moved);
      setPhase(next.phase);
      return next.phase.kind === "idle";
    },
    [store, feature, path, hash],
  );

  return {
    phase,
    settle: () => send({ clear: false, confirm: false }),
    confirm: () => send({ clear: false, confirm: true }),
    unsettle: () => send({ clear: true, confirm: false }),
    dismiss: () => setPhase({ kind: "idle" }),
  };
}
