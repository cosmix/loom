import type { PhrasingContent } from "mdast";
import { fromMarkdown } from "mdast-util-from-markdown";
import { type JSX, useMemo } from "react";

import { renderSummaryInlines } from "@/plan/markdown/inline";

/**
 * A paragraph contributes its inline nodes; a single numbered paragraph also keeps its literal
 * prefix. Other blocks keep their source text, markers included. GFM is not
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
    const item = block.type === "list" && block.ordered ? block.children[0] : undefined;
    const paragraph = item?.children[0];
    const numbered =
      block.type === "list" &&
      block.children.length === 1 &&
      item?.children.length === 1 &&
      paragraph?.type === "paragraph" &&
      /^\d+\. /.test(source);
    const prefixEnd = numbered ? paragraph.position?.start.offset : undefined;
    let children: PhrasingContent[] = source ? [{ type: "text", value: source }] : [];
    if (block.type === "paragraph") children = block.children;
    else if (numbered && start !== undefined && prefixEnd !== undefined) {
      children = [{ type: "text", value: text.slice(start, prefixEnd) }, ...paragraph.children];
    }
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
