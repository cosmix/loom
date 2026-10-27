import type { SourceAnchor } from "@/plan/api/schema-concept";

/** An open run maintained by view-state and carried forward by open-state. */
export type OpenRun = {
  level: 0 | 1 | 2;
  sentenceId: string;
  restsOn: SourceAnchorRef[];
  parent: string | null;
};

/** A source anchor with its last known title, used by open-state. */
export type SourceAnchorRef = SourceAnchor & { title?: string };

/** A note produced by open-state and maintained by view-state. */
export type FoldNote = { paragraphOf: string; text: string; path: string };

/** A focus target produced by reading-order. */
export type FocusTarget = {
  kind: "sentence" | "passage";
  id: string;
  level: 0 | 1 | 2;
  runOf: string | null;
};

/**
 * A reading-order target as the column focuses it. A passage also names the parent of the run
 * that holds it: a sentence cited by two overview sentences can be open under both, and then
 * shows the same passages twice, which only the parent tells apart.
 */
export type ColumnTarget = FocusTarget & { parent?: string | null };
