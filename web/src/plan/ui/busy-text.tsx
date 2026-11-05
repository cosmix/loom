import type { JSX, ReactNode } from "react";

import { BusyRoundel } from "@/aurora-ui/feedback/BusyRoundel";

/**
 * A text-only busy message led by the roundel (`ui/DESIGN.md`, Status). The roundel sits in an
 * `aria-hidden` wrapper, so the words are the only announcement: the caller keeps its own role
 * (`role="status"`, a notice, a row) around this. The disc is sized from the text it leads.
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
      <span className="plan-busy-text__roundel" aria-hidden="true">
        <BusyRoundel busy size="0.9em" busyLabel="" idleLabel="" />
      </span>
      {children}
    </span>
  );
}
