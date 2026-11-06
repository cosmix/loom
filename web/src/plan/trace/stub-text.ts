import type { DocView } from "@/plan/api/schema-files";
import { passageOf } from "@/plan/concept/passages";

/** A stub as the reader sees it: a title and the line or paragraph that sets the scope. */
export interface StubText {
  title: string;
  body: string | null;
}

const MAX_BODY = 400;
/** Edge lines (`Covers: ...`) are the trace's own, not prose a reader wants in a stub. */
const EDGE_LINE = /^(?:Covers|Implements|About|Settled|Source|Status)\b[^:]*:/;

function clip(text: string): string {
  return text.length > MAX_BODY ? `${text.slice(0, MAX_BODY).trimEnd()}…` : text;
}

/**
 * The vision table row of section `number`: its name and what it covers, as plain cell text.
 * Null when the vision holds no such row (the table was edited past the trace).
 */
export function visionRowText(text: string, number: number): StubText | null {
  for (const line of text.split("\n")) {
    if (!line.startsWith("|")) continue;
    const cells = line
      .split("|")
      .slice(1, -1)
      .map((cell) => cell.trim());
    if (/^\d+$/.test(cells[0] ?? "") && Number(cells[0]) === number) {
      return { title: cells[1] ?? "", body: cells[2] ? clip(cells[2]) : null };
    }
  }
  return null;
}

/** The first paragraph under a heading that is prose, not an edge line. */
function firstProse(lines: string[]): string | null {
  const paragraphs = lines
    .join("\n")
    .split(/\n\s*\n/)
    .map((paragraph) => paragraph.trim())
    .filter((paragraph) => paragraph !== "");
  const prose = paragraphs.find((paragraph) => !EDGE_LINE.test(paragraph));
  return prose ? clip(prose.replace(/\s*\n\s*/g, " ")) : null;
}

/** A metaplan entry's heading and first paragraph, read from the metaplan's text. */
export function entryText(doc: DocView, anchor: string): StubText | null {
  const passage = passageOf(doc, anchor);
  if (passage === null) return null;
  const lines = passage.markdown.split("\n");
  const body = lines[0]?.startsWith("#") ? lines.slice(1) : lines;
  return { title: passage.title, body: firstProse(body) };
}
