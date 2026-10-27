import type { AlignType, Table, TableRow } from "mdast";
import type { JSX } from "react";

import type { RenderContext } from "@/plan/markdown/context";
import { renderInlines } from "@/plan/markdown/inline";

function alignClass(align: AlignType | undefined): string | undefined {
  return align ? `md-align-${align}` : undefined;
}

function renderCells(
  row: TableRow,
  Cell: "th" | "td",
  align: readonly AlignType[],
  context: RenderContext,
): JSX.Element[] {
  return row.children.map((cell, index) => (
    <Cell key={index} className={alignClass(align[index])}>
      {renderInlines(cell.children, context)}
    </Cell>
  ));
}

/**
 * A GFM table in ledger ruling. It scrolls sideways inside its own box, so a
 * wide table never makes the page scroll.
 */
export function MarkdownTable({
  node,
  context,
}: {
  node: Table;
  context: RenderContext;
}): JSX.Element {
  const [head, ...body] = node.children;
  const align = node.align ?? [];
  return (
    // Focusable so a keyboard can scroll a wide table.
    <div className="md-table-scroll" role="region" aria-label="Table" tabIndex={0}>
      <table className="md-table">
        {head && (
          <thead>
            <tr>{renderCells(head, "th", align, context)}</tr>
          </thead>
        )}
        <tbody>
          {body.map((row, index) => (
            <tr key={index}>{renderCells(row, "td", align, context)}</tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
