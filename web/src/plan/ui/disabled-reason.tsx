import { useId, type ComponentProps, type ReactElement, type ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { Untrusted } from "@/plan/chat/visible-controls";

/** The three button looks of `ui/design.css` (DESIGN.md, Buttons). Primary is one per screen. */
export type ButtonLook = "quiet" | "secondary" | "primary";

/** Where a blocked control's reason goes: beneath it (the default), or after it on its line. */
export type ReasonPlacement = "below" | "inline";

/** One baseline line that wraps only when too narrow: a button, then what follows it. */
export const INLINE_BASELINE = "inline-flex max-w-full flex-wrap items-baseline gap-x-2 gap-y-1";

const PLACEMENT: Record<ReasonPlacement, string> = {
  below: "flex flex-col items-start gap-1",
  // Inline: the reason follows the button on its baseline and wraps under it only when the line
  // is too narrow. The document header's status line uses it.
  inline: INLINE_BASELINE,
};

/*
 * The planner's button, secondary unless its owner passes `look="primary"`: `.plan-button` in
 * `ui/design.css` sets the scale (28px, 13px/500, one line) over Button's own utilities, and
 * `data-look` picks the look. `size` passes through to Button, whose `data-size="sm"` the
 * stylesheet reads for the compact row variant.
 */
export function DisabledReason({
  reason,
  onClick,
  size,
  look = "secondary",
  reasonPlacement = "below",
  children,
}: {
  /** Text is untrusted; an element is the caller's own markup (e.g. `BusyText`). */
  reason: string | ReactElement | null;
  onClick: () => void;
  size?: ComponentProps<typeof Button>["size"];
  look?: ButtonLook;
  reasonPlacement?: ReasonPlacement;
  children: ReactNode;
}) {
  const reasonId = useId();
  const blocked = reason !== null;
  const Wrapper = reasonPlacement === "inline" ? "span" : "div";

  return (
    <Wrapper className={PLACEMENT[reasonPlacement]}>
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
          {typeof reason === "string" ? <Untrusted text={reason} /> : reason}
        </span>
      )}
    </Wrapper>
  );
}
