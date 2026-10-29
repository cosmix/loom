import type { PhrasingContent } from "mdast";
import { fromMarkdown } from "mdast-util-from-markdown";
import { type JSX, useMemo } from "react";

import { renderSummaryInlines } from "@/plan/markdown/inline";

/**
 * A paragraph contributes its inline nodes; any other block (list, quote, heading, code) keeps
 * its source text, markers included, so the screen shows what `sentence.text` says. GFM is not
 * enabled: summaries use code spans, emphasis and strong only. A link reference definition
 * draws nothing in rendered Markdown, so it draws nothing here.
 */
function summaryInlines(text: string): PhrasingContent[] {
  const nodes: PhrasingContent[] = [];
  for (const block of fromMarkdown(text).children) {
    if (block.type === "definition") continue;
    const start = block.position?.start.offset;
    const end = block.position?.end.offset;
    const source = start === undefined || end === undefined ? "" : text.slice(start, end);
    const children: PhrasingContent[] =
      block.type === "paragraph" ? block.children : source ? [{ type: "text", value: source }] : [];
    if (children.length === 0) continue;
    if (nodes.length > 0) nodes.push({ type: "text", value: " " });
    nodes.push(...children);
  }
  return nodes;
}

export function SentenceText({ text }: { text: string }): JSX.Element {
  const nodes = useMemo(() => summaryInlines(text), [text]);
  return <>{nodes.length > 0 ? renderSummaryInlines(nodes) : text}</>;
}
