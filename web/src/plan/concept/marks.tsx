import type { CSSProperties, JSX } from "react";

import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { ConceptPage, Mark, OutOfDate, Sentence } from "@/plan/api/schema-concept";

/**
 * Colours run 1 to 8 (`--doc-1` to `--doc-8`) and repeat after eight. Colour 0 is the server's
 * "no colour in this scope" (vision, metaplan, out-of-scope citations) and draws muted.
 */
export function docColour(colour: number): string {
  if (colour === 0) return "var(--muted-foreground)";
  return `var(--doc-${((colour + 7) % 8) + 1})`;
}

/** Why a summary is out of date, in the words the mark's tooltip uses. */
export function reasonText(outOfDate: OutOfDate): string {
  switch (outOfDate.reason) {
    case "rebuilding":
      return "rebuilding";
    case "awaiting_approval":
      return "waiting for approval";
    case "cannot_rebuild":
      return outOfDate.detail ? `cannot rebuild: ${outOfDate.detail}` : "cannot rebuild";
    case "failed":
      return "build failed";
    case "source_changed":
      return "source changed, rebuild pending";
  }
}

/** `<title>: <path>`, plus `, out of date: <reason>` for a dashed mark. */
export function markLabel(mark: Mark): string {
  const base = `${mark.doc.title}: ${mark.doc.path}`;
  return mark.out_of_date ? `${base}, out of date: ${reasonText(mark.out_of_date)}` : base;
}

function MarkSquare({ mark, onOpen }: { mark: Mark; onOpen: (page: string) => void }): JSX.Element {
  const dashed = mark.out_of_date !== null;
  const colour = docColour(mark.colour);
  const style = { background: colour, "--mark-colour": colour } as CSSProperties;
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        {/* Tab leaves the column for the chat panel; `d` is the keyboard way to a summary page. */}
        <button
          type="button"
          tabIndex={-1}
          className="concept-mark"
          aria-label={markLabel(mark)}
          data-dashed={dashed ? "true" : undefined}
          style={style}
          onClick={() => onOpen(mark.doc.page)}
        />
      </TooltipTrigger>
      <TooltipContent>
        <span className="flex flex-col">
          <span>{`${mark.doc.title}: ${mark.doc.path}`}</span>
          {mark.out_of_date && <span>{`Out of date: ${reasonText(mark.out_of_date)}`}</span>}
        </span>
      </TooltipContent>
    </Tooltip>
  );
}

/** One small square per document the sentence draws on, after its final punctuation. */
export function Marks({
  marks,
  onOpen,
}: {
  marks: Mark[];
  onOpen: (page: string) => void;
}): JSX.Element | null {
  if (marks.length === 0) return null;
  return (
    <span className="concept-marks">
      {marks.map((mark, index) => (
        <MarkSquare key={`${index}|${mark.doc.id}`} mark={mark} onOpen={onOpen} />
      ))}
    </span>
  );
}

/** A document's own mark, drawn from its summary page when the page's sentences carry none. */
export function pageMark(page: ConceptPage, pageKey: string, summary: string): Mark | null {
  if (!page.path || page.colour === null) return null;
  const doc = { id: summary, title: page.title, path: page.path, page: pageKey };
  return { doc, colour: page.colour, out_of_date: page.state?.out_of_date ?? null };
}

/**
 * The marks of a level 1 sentence opened from `citing`: the citing sentence's mark for that
 * document, else the sentence's own, else its page's. A level 1 sentence carries one mark.
 */
export function citedMarks(
  citing: Sentence,
  summary: string,
  cited: Sentence,
  fallback: Mark | null,
): Mark[] {
  const supplied = citing.marks.filter((mark) => mark.doc.id === summary);
  if (supplied.length > 0) return supplied;
  if (cited.marks.length > 0) return cited.marks;
  return fallback ? [fallback] : [];
}
