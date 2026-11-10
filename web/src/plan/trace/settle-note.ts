import { useAtomValue } from "jotai/react";
import { useEffect, useRef, useState, type RefObject } from "react";

import type { SettleCommitOutcome, SettleCommitStart } from "@/plan/api/schema-trace";
import { connectionEpochAtom } from "@/plan/state/atoms";

import { useSettleCommits } from "./settle-commits";

/**
 * The note beside the control about the last written line's commit, which runs after the answer:
 * running, made (for a few seconds), or what went wrong.
 */
export type CommitNote =
  | { kind: "none" }
  | { kind: "pending" }
  | { kind: "committed"; commit: string }
  | { kind: "failed"; message: string };

/** How long "Committed <commit>" stays beside the control. */
export const COMMITTED_NOTE_MS = 5_000;
/** The note once a reconnect may have dropped the commit's end: frames sent while the socket was
 *  down are not sent again, and a server that restarted dropped its queue. */
export const LOST_NOTE = "The commit result was lost; check git log.";

/** Why a written line was not committed, as the note says it. */
function notCommitted(clear: boolean, reason: string | null): CommitNote {
  const verb = clear ? "Unsettled" : "Settled";
  return {
    kind: "failed",
    message: `${verb}, but not committed: ${reason ?? "git gave no reason"}`,
  };
}

/** The note a commit's end leaves: nothing loud for a commit, the reason for anything else. A
 *  committed outcome's reason is sentences of their own (a hook changed the document). */
function noteOf(outcome: SettleCommitOutcome, clear: boolean): CommitNote {
  const { state, commit, reason } = outcome;
  if (state === "not_committed") return notCommitted(clear, reason);
  if (state === "committed" && reason !== null) {
    const verb = clear ? "Unsettled" : "Settled";
    return { kind: "failed", message: `${verb} and committed. ${reason}` };
  }
  return state === "committed" && commit !== null
    ? { kind: "committed", commit }
    : { kind: "none" };
}

/**
 * The commit a press waits for: `job` once the answer names it; outcomes that came first; the
 * connection epoch it last saw, and `lost` once a reconnect since may have dropped its end. A
 * press while an earlier press's commit still runs waits beside it: the queue ends them in order.
 */
type Waiting = {
  clear: boolean;
  job: string | null;
  early: SettleCommitOutcome[];
  epoch: number;
  lost: boolean;
};

type SetNote = (note: CommitNote) => void;

/** The note while the job runs: running, or lost once a reconnect may have dropped its end. */
const runningNote = (wait: Waiting): CommitNote =>
  wait.lost ? { kind: "failed", message: LOST_NOTE } : { kind: "pending" };

/** A reconnect while presses wait marks their ends as possibly lost, and the note says so once the
 *  last press's job is known; an end, should it arrive after all, still replaces the note. */
function useLostOnReconnect(waiting: RefObject<Waiting[]>, epoch: number, setNote: SetNote) {
  useEffect(() => {
    const stale = waiting.current.filter((wait) => wait.epoch !== epoch);
    for (const wait of stale) Object.assign(wait, { epoch, lost: true });
    const last = waiting.current.at(-1);
    if (last?.lost && last.job !== null) setNote(runningNote(last));
  }, [waiting, epoch, setNote]);
}

/** "Committed <commit>" goes after `COMMITTED_NOTE_MS`. */
function useFadingCommitted(note: CommitNote, setNote: SetNote) {
  useEffect(() => {
    if (note.kind !== "committed") return;
    const timer = setTimeout(() => setNote({ kind: "none" }), COMMITTED_NOTE_MS);
    return () => clearTimeout(timer);
  }, [note, setNote]);
}

/**
 * The commit note of the control of the document at `path`: `expect` starts waiting at a press,
 * `started` takes the answer's commit (`cancel` an answer that wrote nothing), and the
 * `settle_commit` of that job, which may beat the answer, ends it. The note follows the last
 * press; an earlier press's commit that ends meanwhile shows only when it went wrong.
 */
export function useCommitNote(path: string) {
  const [note, setNote] = useState<CommitNote>({ kind: "none" });
  const waiting = useRef<Waiting[]>([]);
  const epoch = useAtomValue(connectionEpochAtom);
  const forget = (wait: Waiting) => {
    waiting.current = waiting.current.filter((other) => other !== wait);
  };
  const end = (outcome: SettleCommitOutcome, wait: Waiting) => {
    const last = waiting.current.at(-1) === wait;
    forget(wait);
    const ended = noteOf(outcome, wait.clear);
    if (last || ended.kind === "failed") setNote(ended);
  };
  useSettleCommits(path, (outcome) => {
    const wait = waiting.current.find((each) => each.job === outcome.job);
    const pressing = waiting.current.at(-1);
    if (wait) end(outcome, wait);
    else if (pressing?.job === null) pressing.early.push(outcome);
  });
  useLostOnReconnect(waiting, epoch, setNote);
  useFadingCommitted(note, setNote);
  /** The last press, while its answer has not named its job. */
  const pressing = () => {
    const last = waiting.current.at(-1);
    return last?.job === null ? last : null;
  };
  const started = (commit: SettleCommitStart) => {
    const wait = pressing();
    if (!wait) return;
    if (commit.state === "not_committed") {
      forget(wait);
      setNote(notCommitted(wait.clear, commit.reason));
      return;
    }
    wait.job = commit.job;
    const early = wait.early.find((outcome) => outcome.job === commit.job);
    if (early) end(early, wait);
    else setNote(runningNote(wait));
  };
  const expect = (clear: boolean) => {
    waiting.current = [...waiting.current, { clear, job: null, early: [], epoch, lost: false }];
    setNote({ kind: "none" });
  };
  const cancel = () => {
    const wait = pressing();
    if (wait) forget(wait);
  };
  return { note, expect, started, cancel };
}
