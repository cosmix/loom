import { parseMarkdown } from "@/plan/markdown/markdown";

import type { DiffBlock } from "./block-diff";

export type DiffSide = "before" | "after";

/** Each side's link reference definitions (`[ref]: url`), as Markdown text in document order. */
export type SideDefinitions = Record<DiffSide, string>;

export const NO_DEFINITIONS: SideDefinitions = { before: "", after: "" };

const DEFINITION_START = /^ {0,3}\[/;

/** Whether a block is only link reference definitions, which Markdown draws as nothing. */
export function isDefinitionBlock(text: string): boolean {
  if (!DEFINITION_START.test(text)) return false;
  const { children } = parseMarkdown(text);
  return children.length > 0 && children.every((node) => node.type === "definition");
}

/** The definitions of each side's document, so a block rendered alone can resolve `[text][ref]`. */
export function definitionsOf(blocks: DiffBlock[]): SideDefinitions {
  const collect = (side: DiffSide): string =>
    blocks
      .map((block) => block[side])
      .filter((text): text is string => text !== null && isDefinitionBlock(text))
      .join("\n\n");
  return { before: collect("before"), after: collect("after") };
}

/** The block's source behind its side's definitions, which render as nothing. */
export function withDefinitions(source: string, definitions: string): string {
  return definitions === "" || !source.includes("[") ? source : `${definitions}\n\n${source}`;
}
