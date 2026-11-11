import { atom, type Atom } from "jotai/vanilla";
import { selectAtom } from "jotai/vanilla/utils";

import type { BackgroundTaskView, TaskState } from "@/plan/api/schema-background";
import { backgroundAtom } from "@/plan/state/atoms";

import { sessionAtom } from "./model";

const NONE: BackgroundTaskView[] = [];

/** A task that can still produce work: `stalled` counts, since it may wake again. */
export function isUnended(state: TaskState): boolean {
  return state === "running" || state === "tool-wait" || state === "stalled";
}

/** The chat session's tasks; a frame for any other session is ignored here. */
export const chatBackgroundAtom = atom((get): BackgroundTaskView[] => {
  const frame = get(backgroundAtom);
  const session = get(sessionAtom);
  return frame !== null && session !== null && frame.session === session.id ? frame.tasks : NONE;
});

export const unendedBackgroundCountAtom = atom(
  (get) => get(chatBackgroundAtom).filter((task) => isUnended(task.state)).length,
);

const entries = new Map<string, Atom<BackgroundTaskView | null>>();

/** The task launched by tool call `toolUseId`, or null. One atom per id, compared by value. */
export function backgroundEntryAtom(toolUseId: string): Atom<BackgroundTaskView | null> {
  let entry = entries.get(toolUseId);
  if (!entry) {
    entry = selectAtom(
      chatBackgroundAtom,
      (tasks) => tasks.find((task) => task.tool_use_id === toolUseId) ?? null,
      sameTask,
    );
    entries.set(toolUseId, entry);
  }
  return entry;
}

function sameTask(a: BackgroundTaskView | null, b: BackgroundTaskView | null): boolean {
  if (a === null || b === null) return a === b;
  // Every field of a task is a primitive, so a shallow compare is a value compare.
  const keys = Object.keys(a) as (keyof BackgroundTaskView)[];
  return keys.every((key) => Object.is(a[key], b[key]));
}
