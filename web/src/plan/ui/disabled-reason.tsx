import { useId, type ComponentProps, type ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { Untrusted } from "@/plan/chat/visible-controls";

/** The three button looks of `ui/design.css` (DESIGN.md, Buttons). Primary is one per screen. */
export type ButtonLook = "quiet" | "secondary" | "primary";

/*
 * The planner's button, secondary unless its owner passes `look="primary"`: `.plan-button` in `ui/design.css` sets the scale (28px, 13px/500, one
 * line) over Button's own utilities, and `data-look` picks the look. `size` passes through to
 * Button, whose `data-size="sm"` the stylesheet reads for the compact row variant.
 */
export function DisabledReason({
  reason,
  onClick,
  size,
  look = "secondary",
  children,
}: {
  reason: string | null;
  onClick: () => void;
  size?: ComponentProps<typeof Button>["size"];
  look?: ButtonLook;
  children: ReactNode;
}) {
  const reasonId = useId();
  const blocked = reason !== null;

  return (
    <div className="flex flex-col items-start gap-1">
      {/* Button styles only `disabled`; this stays focusable, so `aria-disabled` needs its own. */}
      <Button
        type="button"
        size={size}
        data-look={look}
        className="plan-button aria-disabled:cursor-not-allowed aria-disabled:opacity-50"
        aria-disabled={blocked || undefined}
        aria-describedby={blocked ? reasonId : undefined}
        onClick={blocked ? undefined : onClick}
      >
        {children}
      </Button>
      {blocked && (
        <span id={reasonId} className="plan-fine text-muted-foreground">
          {/* The reason can carry a server name or message (`authorBlockReasonAtom`). */}
          <Untrusted text={reason} />
        </span>
      )}
    </div>
  );
}
