import { useStore } from "jotai/react";
import { useCallback, useEffect, useRef, useState } from "react";

import { z } from "@/api/zod";
import { apiPost, type ApiResult } from "@/plan/api/client";
import {
  settleResultSchema,
  unresolvedSchema,
  type SettleCommitStart,
  type SettleState,
} from "@/plan/api/schema-trace";
import { refreshDocView } from "@/plan/concept/passages";
import { docRel } from "@/plan/files/address";
import { docChangedAtom } from "@/plan/files/atoms";
import type { Store } from "@/state/store";

import { useCommitNote } from "./settle-note";
import { loadTrace } from "./state";

/** The route's other refusals that carry a body in 409 or 422 (`stale` while the trace trails). */
const refusalSchema = z.strictObject({ error: z.string(), message: z.string() });

/**
 * `POST /api/settle` answers a settle result (written, with where its commit stands, or nothing
 * written), the 409 `unresolved` answer without `confirm=1`, or a refusal shown as it is.
 */
const settleAnswerSchema = z.union([settleResultSchema, unresolvedSchema, refusalSchema]);
type SettleAnswer = z.infer<typeof settleAnswerSchema>;

/**
 * Where the button stands: at rest, working, released late, asking to confirm open points, or
 * telling why not. Working lasts from the press until the trace shows the document's state
 * changed and the opened document's hash moved off `pressHash` (a press then posts the new hash):
 * `saved` is the hash the write answered, null until then; `from` is the state the press found,
 * `clear` an unsettle. `late` is a written line released after `SETTLE_WAIT_MS` with the trace
 * still at `from`; it ends when the trace arrives.
 */
export type SettlePhase =
  | { kind: "idle" }
  | {
      kind: "working";
      clear: boolean;
      saved: string | null;
      from: SettleState;
      pressHash: string;
    }
  | { kind: "late"; from: SettleState }
  | { kind: "confirm"; myCalls: number; openQuestions: number }
  | { kind: "notice"; message: string };

const CONFLICT_NOTICE =
  "This document changed while you were reading it. It has been reloaded; settle it again if it still reads right.";
const INVALID_NOTICE = "This plan has validation errors. Fix them before settling it.";
export const LATE_NOTICE =
  "Saved, but this page has not caught up yet. Reload the page to see the new state.";

/**
 * How long a written line may wait for the trace to show it; then the control is released with a
 * notice. Revisit if the trace broadcast is ever slower than this on a large repository.
 */
export const SETTLE_WAIT_MS = 10_000;

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

/** What an answer came to: a written line and its commit, or the phase it leaves the button in. */
type Answered =
  | { saved: true; hash: string; commit: SettleCommitStart }
  | { saved: false; phase: SettlePhase; moved: string | null };

function answered(result: ApiResult<SettleAnswer>): Answered {
  const ended = (phase: SettlePhase, moved: string | null = null) =>
    ({ saved: false, phase, moved }) as const;
  if (!result.ok) return ended({ kind: "notice", message: result.message });
  const answer = result.value;
  if ("error" in answer) {
    if (!("my_calls" in answer)) return ended({ kind: "notice", message: answer.message });
    const { my_calls: myCalls, open_questions: openQuestions } = answer;
    return ended({ kind: "confirm", myCalls, openQuestions });
  }
  if (answer.result === "saved") return { saved: true, hash: answer.hash, commit: answer.commit };
  if (answer.result === "conflict") {
    return ended({ kind: "notice", message: CONFLICT_NOTICE }, answer.current_hash);
  }
  return ended({ kind: "notice", message: INVALID_NOTICE });
}

/**
 * Reloads the document the way a save does (its summary page's text and the open file's hash),
 * and the trace, which also arrives by `trace_changed` once the watcher sees the write.
 */
function reload(store: Store, feature: string, path: string, hash: string): void {
  refreshDocView(store, path);
  store.set(docChangedAtom, (current) => ({ ...current, [path]: hash }));
  void loadTrace(store, feature);
}

/**
 * Ends the button's wait: once the trace shows the document's state moved off the press's and the
 * opened hash moved on (`rest`), or as `late` after `SETTLE_WAIT_MS`; a late release ends when the
 * trace arrives.
 */
function useWorkingEnds(
  phase: SettlePhase,
  state: SettleState,
  hash: string | null,
  setPhase: (phase: SettlePhase) => void,
  rest: (phase: SettlePhase) => void,
) {
  useEffect(() => {
    if (phase.kind === "late" && state !== phase.from) setPhase({ kind: "idle" });
    if (phase.kind !== "working" || phase.saved === null) return;
    const moved = hash !== phase.pressHash || phase.saved === phase.pressHash;
    if (state !== phase.from && moved) {
      rest({ kind: "idle" });
      return;
    }
    const timer = setTimeout(() => rest({ kind: "late", from: phase.from }), SETTLE_WAIT_MS);
    return () => clearTimeout(timer);
  }, [phase, state, hash, setPhase, rest]);
}

/**
 * The settle flow of one document. `state` is its settle state as the trace shows it now and
 * `hash` the opened document's hash: a press keeps the button working until both have moved on
 * (the written line arrived, so the next press posts the new hash); an answer other than saved
 * ends the wait, and so does `SETTLE_WAIT_MS`. A second press while working is ignored at once,
 * before any re-render could disable the button. The commit runs after the answer; its `note`
 * outlives the wait.
 */
export function useSettle(feature: string, path: string, hash: string | null, state: SettleState) {
  const store = useStore();
  const [phase, setPhase] = useState<SettlePhase>({ kind: "idle" });
  const busy = useRef(false);
  const commit = useCommitNote(path);
  const rest = useCallback((next: SettlePhase) => {
    busy.current = false;
    setPhase(next);
  }, []);
  useWorkingEnds(phase, state, hash, setPhase, rest);

  /** Resolves true when the line was written, so the caller can move focus off the button. */
  const send = async (flags: { clear: boolean; confirm: boolean }): Promise<boolean> => {
    if (hash === null || busy.current) return false;
    busy.current = true;
    const working = { kind: "working", clear: flags.clear, from: state, pressHash: hash } as const;
    setPhase({ ...working, saved: null });
    commit.expect(flags.clear);
    const next = answered(await postSettle(path, hash, flags));
    if (!next.saved) {
      if (next.moved !== null) reload(store, feature, path, next.moved);
      commit.cancel();
      rest(next.phase);
      return false;
    }
    reload(store, feature, path, next.hash);
    setPhase({ ...working, saved: next.hash });
    commit.started(next.commit);
    return true;
  };

  return {
    phase,
    note: commit.note,
    settle: () => send({ clear: false, confirm: false }),
    confirm: () => send({ clear: false, confirm: true }),
    unsettle: () => send({ clear: true, confirm: false }),
    // Only a confirmation or notice is dismissed; work in flight runs to its end.
    dismiss: () => setPhase((now) => (now.kind === "working" ? now : { kind: "idle" })),
  };
}
