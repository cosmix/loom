import { cn } from "cn";
import type { JSX } from "react";
import { Link } from "react-router";

import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";

import type { HeadingChip, HeadingTone } from "./doc-heading";

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

/** The chip's words: its label, then its inline links. */
function ChipWords({ chip }: { chip: HeadingChip }): JSX.Element {
  return (
    <span className="doc-heading-chip__label">
      <Described label={chip.label} description={chip.description} />
      {chip.links && (
        <>
          {" "}
          <ChipLinks chip={chip} />
        </>
      )}
    </span>
  );
}

/**
 * A status: a link, a pressable button or a plain span (`.doc-heading-chip`) carrying the
 * tooltip and the tone, holding its glyph column and its words. The glyph column is drawn for a
 * chip without a glyph too, so every row's words share one left edge; only the glyph takes a
 * tone, and no fill or pill is drawn (DESIGN.md, Status).
 */
export function Chip({ chip }: { chip: HeadingChip }): JSX.Element {
  const tone = chip.tone ?? "neutral";
  const shared = { title: hoverTitle(chip.title), "data-chip": "", "data-tone": tone };
  const content = (
    <>
      <span aria-hidden="true" className={cn("doc-heading-chip__glyph", CHIP_ICON[tone])}>
        {chip.icon}
      </span>
      <ChipWords chip={chip} />
    </>
  );
  if (chip.to !== undefined) {
    return (
      <Link to={chip.to} {...shared} className={cn("doc-heading-chip", FOCUS, "rounded")}>
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
        className={cn("doc-heading-chip", FOCUS, "rounded")}
      >
        {content}
      </button>
    );
  }
  return (
    <span {...shared} className="doc-heading-chip">
      {content}
    </span>
  );
}

/**
 * The status card (`.doc-heading-status`, `doc-heading.css`): one row per chip, in the order
 * given, each row its status, then its detail line under the words and its action at the end.
 */
export function StatusCard({ chips }: { chips: readonly HeadingChip[] }): JSX.Element {
  return (
    <ul className="doc-heading-status plan-surface" aria-label="Status">
      {chips.map((chip) => (
        <li key={chip.key} className="doc-heading-row" data-action={chip.action ? "" : undefined}>
          <Chip chip={chip} />
          {chip.detail && <div className="doc-heading-row__detail plan-fine">{chip.detail}</div>}
          {chip.action && <div className="doc-heading-row__action">{chip.action}</div>}
        </li>
      ))}
    </ul>
  );
}

/**
 * The facts line (`.doc-heading-facts`): what the document relates to, as label and value pairs
 * ("Built by 05 · 09", "Implements §01, §03"). A fact keeps the chip's tooltip and tone attributes
 * on its pair, so its text and links read as the chip's do.
 */
export function FactsLine({ chips }: { chips: readonly HeadingChip[] }): JSX.Element {
  return (
    <dl className="doc-heading-facts">
      {chips.map((chip) => (
        <div
          key={chip.key}
          className="doc-heading-fact"
          title={hoverTitle(chip.title)}
          data-chip=""
          data-tone={chip.tone ?? "neutral"}
        >
          <dt>
            <Described label={chip.label} description={chip.description} />
          </dt>
          {chip.links && chip.links.length > 0 && (
            <>
              {" "}
              <dd>
                <ChipLinks chip={chip} />
              </dd>
            </>
          )}
        </div>
      ))}
    </dl>
  );
}
