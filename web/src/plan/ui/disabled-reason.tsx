import { useId, type ComponentProps, type ReactElement, type ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { Untrusted } from "@/plan/chat/visible-controls";
import "./hearth.css";

/** The three button looks of `ui/design.css` (DESIGN.md, Buttons). Primary is one per screen. */
export type ButtonLook = "quiet" | "secondary" | "primary";

/**
 * The hearth shapes of `ui/hearth.css` (DESIGN.md, Chat), drawn only in the chat and on `/new`:
 * `pill` is the full-radius 32px button, `round` the same height squared for a glyph alone.
 */
export type ButtonShape = "pill" | "round";

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
 * stylesheet reads for the compact row variant; `data-shape` picks a hearth shape over both.
 */
export function DisabledReason({
  reason,
  reasonId,
  onClick,
  size,
  look = "secondary",
  shape,
  reasonPlacement = "below",
  describedBy,
  label,
  expanded,
  controls,
  busy = false,
  children,
}: {
  /** Text is untrusted; an element is the caller's own markup (e.g. `BusyText`). */
  reason: string | ReactElement | null;
  /**
   * The id of an element the caller draws the reason in (the chat composer's foot line, shared by
   * the notes beside it). The button is described by it and no reason is drawn here, so the
   * reason is still drawn once; the caller keeps that element in the DOM while `reason` is set.
   */
  reasonId?: string;
  onClick: () => void;
  size?: ComponentProps<typeof Button>["size"];
  look?: ButtonLook;
  shape?: ButtonShape;
  reasonPlacement?: ReasonPlacement;
  /** The id of another element that describes the button (e.g. the text it acts on); it follows the reason's id. */
  describedBy?: string;
  /** The accessible name when it must say more than the visible text; it starts with that text. */
  label?: string;
  /** For a button that opens a region inline: whether it is open (`aria-expanded`). */
  expanded?: boolean;
  /** The id of the region it opens (`aria-controls`); pass it only while that region is drawn. */
  controls?: string;
  /** Work this button started is running: it stays focusable but inert, and draws no reason (its label says so). */
  busy?: boolean;
  children: ReactNode;
}) {
  const ownId = useId();
  const blocked = reason !== null;
  const inert = blocked || busy;
  const described = [blocked ? (reasonId ?? ownId) : undefined, describedBy]
    .filter((id): id is string => id !== undefined && id !== "")
    .join(" ");
  const Wrapper = reasonPlacement === "inline" ? "span" : "div";

  return (
    <Wrapper className={PLACEMENT[reasonPlacement]}>
      {/* Button styles only `disabled`; this stays focusable, so `aria-disabled` needs its own. */}
      <Button
        type="button"
        size={size}
        data-look={look}
        data-shape={shape}
        className="plan-button aria-disabled:cursor-not-allowed aria-disabled:opacity-50"
        aria-disabled={inert || undefined}
        aria-busy={busy || undefined}
        aria-label={label}
        aria-expanded={expanded}
        aria-controls={controls}
        aria-describedby={described === "" ? undefined : described}
        onClick={inert ? undefined : onClick}
      >
        {children}
      </Button>
      {blocked && reasonId === undefined && (
        <span id={ownId} className="plan-fine text-muted-foreground">
          {/* The reason can carry a server name or message (`authorBlockReasonAtom`). */}
          {typeof reason === "string" ? <Untrusted text={reason} /> : reason}
        </span>
      )}
    </Wrapper>
  );
}
