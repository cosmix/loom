import type { Item } from "@/plan/api/schema-chat";

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
  if (items.length < 3) return items;
  const paths = new Set<string>();
  for (const item of items) {
    const path = editedPath(item);
    if (path !== null) paths.add(path);
  }
  return [
    { kind: "group", items, label: `Used ${items.length} tools · edited ${paths.size} files` },
  ];
}

export function groupToolRows(items: Item[], turnEnded: boolean): Array<Item | ToolGroup> {
  if (!turnEnded) return items;
  const rows: Array<Item | ToolGroup> = [];
  let run: ToolItem[] = [];
  for (const item of items) {
    if (
      item.type === "tool" &&
      item.subagent === null &&
      item.status !== "error" &&
      item.status !== "denied"
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
