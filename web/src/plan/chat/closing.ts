import { z } from "@/api/zod";
import type { Item, Turn } from "@/plan/api/schema-chat";
import { fenceBody } from "./kickoff-fence";

type ToolItem = Extract<Item, { type: "tool" }>;
export type ClosingTick = { path: string; state: "done" | "live" };
export type LeftBehind = {
  documents: Array<{ path: string; added: number | null; removed: number | null }>;
  decisions: string[];
  questions: string[];
};

const CLOSING_INFO = "loom-closing";
const WRITE_TOOLS = new Set(["Edit", "Write", "MultiEdit"]);

/** The closing block is read as data: any other key is ignored, long lists are refused whole. */
const MAX_ENTRIES = 50;
const MAX_ENTRY_CHARS = 500;
const closingEntries = z.array(z.string().max(MAX_ENTRY_CHARS)).max(MAX_ENTRIES);
const closingBlock = z.object({ decisions: closingEntries, questions: closingEntries });

/** A turn that opens with the closing prompt loom plan sends when the chat ends. */
export function isClosingTurn(turn: Turn): boolean {
  return turn.items.some((item) => item.type === "user" && item.origin === "closing");
}

/** The user wrote to the model: the server's rule for whether the closing write runs at all. */
export function userWrote(turns: Turn[]): boolean {
  return turns.some((turn) =>
    turn.items.some(
      (item) => item.type === "user" && item.origin === null && item.delivery !== "undelivered",
    ),
  );
}

function documentWrites(items: Item[]): ToolItem[] {
  return items.filter(
    (item): item is ToolItem =>
      item.type === "tool" && WRITE_TOOLS.has(item.name) && item.summary !== "",
  );
}

/** The documents the closing turn is writing, in the order it reached them. */
export function closingTicks(turn: Turn): ClosingTick[] {
  const ticks = new Map<string, ClosingTick["state"]>();
  for (const item of documentWrites(turn.items)) {
    if (item.status !== "pending" && item.status !== "done") continue;
    const state = item.status === "done" ? "done" : "live";
    if (ticks.get(item.summary) !== "done") ticks.set(item.summary, state);
  }
  return [...ticks].map(([path, state]) => ({ path, state }));
}

function closingLists(closing: Turn | undefined): Pick<LeftBehind, "decisions" | "questions"> {
  const empty = { decisions: [], questions: [] };
  const bodies = (closing?.items ?? []).flatMap((item) =>
    item.type === "text" ? [fenceBody(item.markdown, CLOSING_INFO)] : [],
  );
  const body = bodies.filter((entry): entry is string => entry !== null).at(-1);
  if (body === undefined) return empty;
  let parsed: unknown;
  try {
    parsed = JSON.parse(body);
  } catch {
    return empty;
  }
  const block = closingBlock.safeParse(parsed);
  if (!block.success) return empty;
  const clean = (entries: string[]) => entries.map((entry) => entry.trim()).filter(Boolean);
  return { decisions: clean(block.data.decisions), questions: clean(block.data.questions) };
}

function lineCounts(turns: Turn[], path: string): { added: number | null; removed: number | null } {
  let added: number | null = null;
  let removed: number | null = null;
  for (const item of turns.flatMap((turn) => documentWrites(turn.items))) {
    if (item.status !== "done" || item.summary !== path) continue;
    if (item.added !== null) added = (added ?? 0) + item.added;
    if (item.removed !== null) removed = (removed ?? 0) + item.removed;
  }
  return { added, removed };
}

/** What a chat left behind, from its turns in order: documents it changed, decisions, open questions. */
export function leftBehind(turns: Turn[]): LeftBehind {
  const paths = new Set(turns.flatMap((turn) => turn.changed_files.map((file) => file.path)));
  const documents = [...paths].map((path) => ({ path, ...lineCounts(turns, path) }));
  const closing = turns.find(isClosingTurn);
  return { documents, ...closingLists(closing) };
}
