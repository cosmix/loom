import { cn } from "cn";
import type { CSSProperties, JSX } from "react";

import { ArrowSpinner } from "./spinner";
import { DOC_KIND_GLYPHS, type DocKind } from "./glyphs";

export const DOC_KIND_COLOUR: Record<DocKind, number> = {
  vision: 1,
  section: 2,
  note: 3,
  metaplan: 4,
  plan: 5,
  brief: 6,
  knowledge: 7,
  review: 8,
};

export function DocMark({
  kind,
  size = "sm",
  rebuilding = false,
  className,
}: {
  kind: DocKind;
  size?: "sm" | "lg";
  rebuilding?: boolean;
  className?: string;
}): JSX.Element {
  const Glyph = DOC_KIND_GLYPHS[kind];
  const colour = `var(--doc-${DOC_KIND_COLOUR[kind]})`;
  const glyphClassName = size === "lg" ? "size-4" : "size-3";

  return (
    <span
      aria-hidden="true"
      className={cn(
        "inline-flex shrink-0 items-center justify-center",
        // The fill is the document colour at 16% (20% in the dark themes); the glyph keeps it whole.
        "bg-[color-mix(in_oklch,var(--doc-mark)_16%,transparent)] dark:bg-[color-mix(in_oklch,var(--doc-mark)_20%,transparent)]",
        size === "lg" ? "size-[30px] rounded-[8px]" : "size-5 rounded-[5px]",
        className,
      )}
      style={{ color: colour, "--doc-mark": colour } as CSSProperties}
    >
      {rebuilding ? (
        // A rebuild is an operation that ends: the arrow spinner in the kind glyph's place.
        <ArrowSpinner data-kind="summaries" className={glyphClassName} />
      ) : (
        <Glyph className={glyphClassName} />
      )}
    </span>
  );
}
