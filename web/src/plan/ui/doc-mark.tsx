import { cn } from "cn";
import type { JSX } from "react";

import { ActivityGlyph } from "./activity-glyphs";
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
        "inline-flex shrink-0 items-center justify-center rounded",
        size === "lg" ? "size-[30px]" : "size-5",
        className,
      )}
      style={{ color: colour, backgroundColor: `color-mix(in oklch, ${colour} 14%, transparent)` }}
    >
      {rebuilding ? (
        <ActivityGlyph kind="summaries" className={glyphClassName} />
      ) : (
        <Glyph className={glyphClassName} />
      )}
    </span>
  );
}
