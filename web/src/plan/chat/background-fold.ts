import type { BackgroundTaskView } from "@/plan/api/schema-background";
import type { SessionView } from "@/plan/api/schema-chat";

import { isUnended } from "./background-tasks";

const WORDS: Record<BackgroundTaskView["state"], string> = {
  running: "running",
  // A task waiting on its own tool call is still at work.
  "tool-wait": "running",
  // The question mark is the doubt: a quiet task may wake again.
  stalled: "stalled?",
  done: "done",
  failed: "failed",
  stopped: "stopped",
};

/** The state word of a background task, in the head's list and on the tool row that launched it. */
export function backgroundWord(state: BackgroundTaskView["state"]): string {
  return WORDS[state];
}

/**
 * Whether a head draws the background line. An idle chat says nothing (`ui/DESIGN.md`, Chat) unless
 * it waits on a task that has not ended. A chat that works or waits on a permission keeps the line
 * too: a draft team's lead turns "working" each time a teammate reports, and the list of running
 * agents must not vanish between those turns. Starting, ending, ended and failed chats leave it
 * out, since their own words stand for the whole session.
 */
export function waitsOnBackground(session: SessionView | null, unended: number): boolean {
  if (unended === 0 || session === null) return false;
  return session.state === "idle" || session.state === "working" || session.state === "permission";
}

/** One line of the head's list: a task, or the ended agents counted together. */
export type BackgroundEntry =
  | { kind: "task"; task: BackgroundTaskView }
  | { kind: "earlier"; count: number };

/**
 * The list's lines, in the table's order: every task that has not ended and every ended shell is a
 * row, and the ended agents fold into one closing "N earlier agents" line. A shell's outcome
 * (failed, stopped) is news the transcript may not carry, since an adopted shell has no tool row;
 * an ended agent's report is in the transcript, on the row that launched it.
 */
export function foldBackground(tasks: readonly BackgroundTaskView[]): BackgroundEntry[] {
  const rows = tasks
    .filter((task) => isUnended(task.state) || task.kind === "shell")
    .map((task): BackgroundEntry => ({ kind: "task", task }));
  const earlier = tasks.length - rows.length;
  return earlier > 0 ? [...rows, { kind: "earlier", count: earlier }] : rows;
}
