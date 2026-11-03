import { cn } from "cn";
import type { JSX } from "react";
import { Link } from "react-router";

import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";

import type { HeadingChip, HeadingTone } from "./doc-heading";
import { INLINE_BASELINE } from "./disabled-reason";

export const FOCUS =
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring";
/** A fill by tone (go, warn, danger wash 18%) for a card's status chip. Heading chips take none. */
export const CHIP_FILL: Record<HeadingTone, string> = {
  neutral: "bg-muted",
  ok: "bg-muted",
  warning: "bg-[color-mix(in_oklch,var(--tone-warning)_18%,transparent)]",
  danger: "bg-[color-mix(in_oklch,var(--tone-blocked)_18%,transparent)]",
  accent: "bg-[color-mix(in_oklch,var(--tone-executing)_18%,transparent)]",
};
export const CHIP_ICON: Record<HeadingTone, string> = {
  neutral: "text-muted-foreground",
  ok: "text-(--tone-completed)",
  warning: "text-(--tone-warning)",
  danger: "text-(--tone-blocked)",
  accent: "text-(--tone-executing)",
};

function Described({ label, description }: { label: string; description?: string }): JSX.Element {
  return (
    <>
      <Untrusted text={label} />
      {description && (
        <>
          {" "}
          <span className="sr-only">
            (<Untrusted text={description} />)
          </span>
        </>
      )}
    </>
  );
}

/** A tooltip carries server text, so its format characters show as code points. */
function hoverTitle(title: string | undefined): string | undefined {
  return title === undefined ? undefined : visibleControls(title);
}

function ChipLinks({ chip }: { chip: HeadingChip }): JSX.Element {
  const separator = chip.linksSeparator ?? ", ";
  return (
    <>
      {chip.links?.map((link, index) => (
        <span key={`${index}:${link.label}`}>
          {index > 0 && <span aria-hidden="true">{separator}</span>}
          {link.to === undefined ? (
            <span title={hoverTitle(link.title)}>
              <Described label={link.label} description={link.description} />
            </span>
          ) : (
            <Link
              to={link.to}
              title={hoverTitle(link.title)}
              className={cn(FOCUS, "rounded underline-offset-2 hover:underline")}
            >
              <Described label={link.label} description={link.description} />
            </Link>
          )}
        </span>
      ))}
    </>
  );
}

function ChipContent({ chip }: { chip: HeadingChip }): JSX.Element {
  return (
    <>
      {chip.icon && (
        <span
          aria-hidden="true"
          className={cn(
            "inline-flex h-5 shrink-0 items-center self-start [&_svg]:size-3.5",
            CHIP_ICON[chip.tone ?? "neutral"],
          )}
        >
          {chip.icon}
        </span>
      )}
      {/* The words, not the glyph box, give the chip its baseline, so an action set after the
          chip (`after`) lines up with the words. */}
      <span className="min-w-0 self-baseline [overflow-wrap:anywhere]">
        <Described label={chip.label} description={chip.description} />
        {chip.links && (
          <>
            {" "}
            <ChipLinks chip={chip} />
          </>
        )}
      </span>
    </>
  );
}

function Chip({ chip }: { chip: HeadingChip }): JSX.Element {
  const tone = chip.tone ?? "neutral";
  // Status is a toned glyph and a word in the text's colour: no fill, no pill (DESIGN.md, Status).
  // The glyph sits in a box as tall as one line and stays on the first line when the words wrap.
  const className =
    "inline-flex min-h-6 max-w-full items-start gap-1.5 text-[13px] leading-5 text-foreground";
  const shared = { title: hoverTitle(chip.title), "data-chip": "", "data-tone": tone };
  const content = <ChipContent chip={chip} />;
  if (chip.to !== undefined) {
    return (
      <Link
        to={chip.to}
        {...shared}
        className={cn(className, FOCUS, "rounded underline-offset-2 hover:underline")}
      >
        {content}
      </Link>
    );
  }
  if (chip.onClick) {
    return (
      <button
        type="button"
        onClick={chip.onClick}
        aria-pressed={chip.pressed}
        {...shared}
        className={cn(className, FOCUS, "rounded underline-offset-2 hover:underline")}
      >
        {content}
      </button>
    );
  }
  return (
    <span {...shared} className={className}>
      {content}
    </span>
  );
}

/** The chip row; a chip with an action keeps it on its own baseline, wrapping as one pair. */
export function ChipRow({ chips }: { chips: readonly HeadingChip[] }): JSX.Element {
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-1">
      {chips.map((chip) =>
        chip.after ? (
          <span key={chip.key} className={INLINE_BASELINE}>
            <Chip chip={chip} />
            {chip.after}
          </span>
        ) : (
          <Chip key={chip.key} chip={chip} />
        ),
      )}
    </div>
  );
}
