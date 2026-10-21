import { cn } from "cn";

import type { StageStatus } from "@/api/schema";
import { stateMeta, type Tone } from "@/lib/format";
import { STAGE_STATES } from "@/lib/states";

/// The one place a tone becomes a CSS class; the colours live in index.css.
export function toneClass(tone: Tone): string {
  return `tone-${tone}`;
}

/// The state glyph alone, for lines where the label is carried by the text.
export function StateGlyph({ status, className }: { status: StageStatus; className?: string }) {
  const { icon: Icon, label } = STAGE_STATES[status];
  const { tone } = stateMeta(status);
  return (
    <span
      aria-label={label}
      role="img"
      className={cn(
        "inline-flex shrink-0 items-center justify-center align-middle",
        toneClass(tone),
        className,
      )}
    >
      <Icon
        aria-hidden="true"
        className="size-[1em]"
        fill={status === "executing" ? "currentColor" : "none"}
      />
    </span>
  );
}

/// Glyph plus label, toned as the TUI's STATE column.
export function StateBadge({ status, className }: { status: StageStatus; className?: string }) {
  const { icon: Icon, label } = STAGE_STATES[status];
  const { tone, bold } = stateMeta(status);
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 whitespace-nowrap",
        toneClass(tone),
        bold && "font-semibold",
        className,
      )}
    >
      <span aria-hidden="true" className="inline-flex shrink-0 items-center justify-center">
        <Icon className="size-[1em]" fill={status === "executing" ? "currentColor" : "none"} />
      </span>
      <span>{label}</span>
    </span>
  );
}
