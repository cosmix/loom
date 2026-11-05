import type { JSX, ReactNode } from "react";

import { RoundelGlyph } from "@/plan/ui/roundel-glyph";

/**
 * A text-only busy message led by the roundel (`ui/DESIGN.md`, Status). The roundel is the
 * decorative glyph (`aria-hidden`), so the words are the only announcement: the caller keeps its
 * own role (`role="status"`, a notice, a row) around this. The disc is sized from the text it
 * leads.
 */
export function BusyText({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}): JSX.Element {
  return (
    <span className={`plan-busy-text${className ? ` ${className}` : ""}`}>
      {/* The disc fills 86% of its box (roundel-glyph.css), so this box draws it at 0.9em. */}
      <RoundelGlyph className="plan-busy-text__roundel" size="1.05em" />
      {children}
    </span>
  );
}
