import type { List, ListItem, PhrasingContent, RootContent } from "mdast";
import { Fragment, type JSX, type ReactNode } from "react";

import { CodeBlock } from "@/plan/markdown/code-block";
import { rawHtmlText, type RenderContext } from "@/plan/markdown/context";
import { Heading } from "@/plan/markdown/heading";
import { renderInlines } from "@/plan/markdown/inline";
import { MarkdownTable } from "@/plan/markdown/table";

function headingAnchor(line: number | undefined, context: RenderContext): string | null {
  if (!context.headingIds || line === undefined) return null;
  return context.headingAnchor(line);
}

/**
 * A suppressed title still leaves its server anchor behind, so links and scroll
 * restore to it find an (empty, zero-size) target.
 */
function SuppressedAnchor({ anchor }: { anchor: string | null }): JSX.Element | null {
  return anchor === null ? null : <span id={anchor} />;
}

function ListItemView(props: {
  item: ListItem;
  tight: boolean;
  context: RenderContext;
}): JSX.Element {
  const { item, tight, context } = props;
  const task = item.checked === true || item.checked === false;
  return (
    <li className={task ? "md-task" : undefined}>
      {task && <input type="checkbox" checked={item.checked === true} disabled readOnly />}
      {renderBlockNodes(item.children, context, tight)}
    </li>
  );
}

function ListView({ node, context }: { node: List; context: RenderContext }): JSX.Element {
  // A list is loose (its paragraphs keep their <p>) when any item is spread.
  const tight = !(node.spread || node.children.some((item) => item.spread));
  const items = node.children.map((item, index) => (
    <ListItemView key={index} item={item} tight={tight} context={context} />
  ));
  if (node.ordered) return <ol start={node.start ?? undefined}>{items}</ol>;
  return (
    <ul className={node.children.some((item) => item.checked != null) ? "md-tasks" : undefined}>
      {items}
    </ul>
  );
}

function renderBlock(node: RootContent, context: RenderContext, tight: boolean): ReactNode {
  switch (node.type) {
    case "paragraph":
      if (tight) return renderInlines(node.children, context);
      return <p>{renderInlines(node.children, context)}</p>;
    case "heading":
      return (
        <Heading
          depth={node.depth}
          anchor={headingAnchor(node.position?.start.line, context)}
          path={context.path}
        >
          {renderInlines(node.children, context)}
        </Heading>
      );
    case "thematicBreak":
      return <hr />;
    case "blockquote":
      return <blockquote>{renderBlockNodes(node.children, context)}</blockquote>;
    case "list":
      return <ListView node={node} context={context} />;
    case "listItem":
      return <ListItemView item={node} tight={tight} context={context} />;
    case "code":
      return <CodeBlock value={node.value} lang={node.lang ?? null} />;
    case "html": {
      const text = rawHtmlText(node.value);
      return text ? <p className="md-raw">{text}</p> : null;
    }
    case "table":
      return <MarkdownTable node={node} context={context} />;
    case "footnoteDefinition":
      return (
        <div className="md-footnote">
          <span className="md-footnote-label">[{node.label ?? node.identifier}]</span>
          {renderBlockNodes(node.children, context)}
        </div>
      );
    case "definition":
    case "tableRow":
    case "tableCell":
    case "yaml":
      return null;
    default:
      return renderInlines([node], context);
  }
}

function renderBlockNodes(
  nodes: readonly RootContent[],
  context: RenderContext,
  tight = false,
  suppressedIndex = -1,
): ReactNode[] {
  return nodes.map((node, index) => (
    <Fragment key={index}>
      {index === suppressedIndex ? (
        <SuppressedAnchor anchor={headingAnchor(node.position?.start.line, context)} />
      ) : (
        renderBlock(node, context, tight)
      )}
    </Fragment>
  ));
}

function headingText(nodes: readonly PhrasingContent[]): string {
  return nodes
    .map((node) => {
      if (node.type === "text" || node.type === "inlineCode") return node.value;
      if ("children" in node) return headingText(node.children);
      if (node.type === "break") return " ";
      if (node.type === "image" || node.type === "imageReference") return node.alt ?? "";
      return "";
    })
    .join("");
}

/** `headingText` yields a code span's value without its backticks, so a title compared with it drops them too. */
export function normalizeTitle(title: string): string {
  return title.replaceAll("`", "").trim().replace(/\s+/g, " ");
}

/** Renders mdast nodes to React elements; `tight` drops the <p> of a tight list's paragraphs. */
export function renderBlocks(
  nodes: readonly RootContent[],
  context: RenderContext,
  tight = false,
): ReactNode[] {
  const title = normalizeTitle(context.suppressTitle ?? "");
  const firstH1 = nodes.findIndex((node) => node.type === "heading" && node.depth === 1);
  const heading = nodes[firstH1];
  const suppressedIndex =
    title && heading?.type === "heading" && normalizeTitle(headingText(heading.children)) === title
      ? firstH1
      : -1;
  // Nested blocks bypass this entry point so their headings always remain visible.
  return renderBlockNodes(nodes, context, tight, suppressedIndex);
}
