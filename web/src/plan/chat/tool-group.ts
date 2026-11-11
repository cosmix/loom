import type { BackgroundTaskView } from "@/plan/api/schema-background";
import type { Item } from "@/plan/api/schema-chat";

import { selectAtom } from "jotai/vanilla/utils";

import { chatBackgroundAtom, isUnended } from "./background-tasks";

/**
 * The chat's tasks that have not ended. The array keeps its identity while the same tool calls are
 * unended, so the transcript regroups when a row starts or stops working, not on every refresh.
 */
export const unendedTasksAtom = selectAtom(
  chatBackgroundAtom,
  (tasks) => tasks.filter((task) => isUnended(task.state)),
  (a, b) => a.length === b.length && a.every((task, i) => task.tool_use_id === b[i]!.tool_use_id),
);

export type ToolGroup = { kind: "group"; items: Item[]; label: string };

type ToolItem = Extract<Item, { type: "tool" }>;

function editedPath(item: ToolItem): string | null {
  if (item.name !== "Edit" && item.name !== "Write") return null;
  try {
    const input: unknown = JSON.parse(item.input);
    if (typeof input !== "object" || input === null || !("file_path" in input)) return null;
    return typeof input.file_path === "string" && input.file_path.length > 0
      ? input.file_path
      : null;
  } catch {
    return null;
  }
}

function foldRun(items: ToolItem[]): Array<Item | ToolGroup> {
  if (items.length === 0) return [];
  const paths = new Set<string>();
  for (const item of items) {
    const path = editedPath(item);
    if (path !== null) paths.add(path);
  }
  const tools = items.length === 1 ? "tool" : "tools";
  return [
    {
      kind: "group",
      items,
      label: `Used ${items.length} ${tools} · edited ${paths.size} ${paths.size === 1 ? "file" : "files"}`,
    },
  ];
}

/** The Skill tool only loads instructions for the agent; its row tells the reader nothing. */
function isSkillRow(item: Item): boolean {
  return item.type === "tool" && item.name === "Skill";
}

/**
 * Every run of one or more plain tool rows folds into one group, while the turn is open and after
 * it ends. Errors, denials, subagent rows and everything else that is not a tool stay outside,
 * and Skill rows are dropped. A row whose background task has not ended stays outside too, so the
 * reader sees it work; once the task ends the row folds like any finished tool.
 */
export function groupToolRows(
  items: Item[],
  background: readonly BackgroundTaskView[],
): Array<Item | ToolGroup> {
  const unended = new Set(
    background.filter((task) => isUnended(task.state)).map((task) => task.tool_use_id),
  );
  const rows: Array<Item | ToolGroup> = [];
  let run: ToolItem[] = [];
  for (const item of items) {
    if (isSkillRow(item)) continue;
    if (
      item.type === "tool" &&
      item.subagent === null &&
      item.status !== "error" &&
      item.status !== "denied" &&
      !unended.has(item.tool_use_id)
    ) {
      run.push(item);
    } else {
      // Nested tools remain outside folds for the Agent row to render under its parent.
      rows.push(...foldRun(run), item);
      run = [];
    }
  }
  rows.push(...foldRun(run));
  return rows;
}
