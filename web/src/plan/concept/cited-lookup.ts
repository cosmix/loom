import type { Sentence, SourceAnchor } from "@/plan/api/schema-concept";

/*
 * How the concept modules name what a sentence rests on and cites. These live apart from
 * open-state.ts because fragments.ts and column-context.ts need them and open-state.ts imports
 * fragments.ts: importing them from open-state.ts would close an import cycle.
 */

/**
 * Resolves a sentence citation (`{ summary, id }`) to the cited sentence of another loaded
 * page. An overview's level-1 sentences live on the summary pages it cites, so carrying a
 * nested level-1 run needs their `rests_on`; without a lookup the run keeps its id when the
 * parent still cites it.
 */
export type CitedLookup = (summary: string, id: string) => Sentence | undefined;

export function anchorKey(anchor: SourceAnchor): string {
  return `${anchor.path}#${anchor.anchor}`;
}
