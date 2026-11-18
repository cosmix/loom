import type { JSX, ReactNode } from "react";

import { ArrowSpinner } from "@/plan/ui/spinner";

/**
 * An operation's busy words led by the small rotating-arrow spinner (`ui/DESIGN.md`, Status), for
 * a control or a line that waits on an action that will end ("Sending…"). The arrow is decorative
 * (`aria-hidden`), so the words are the only announcement: the caller keeps its own role
 * (`role="status"`, a notice, a row) around this. The arrow is sized from the text (1em).
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
      <ArrowSpinner size="1em" />
      {children}
    </span>
  );
}
