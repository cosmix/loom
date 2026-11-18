import { cn } from "cn";
import type { JSX } from "react";
import { Link } from "react-router";

import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";

import type { HeadingChip, HeadingChipLink, HeadingTone } from "./doc-heading";
import "./doc-heading-links.css";

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

/** A link chip's words: the number or title, the title after a number, then the state. */
function LinkWords({ link }: { link: HeadingChipLink }): JSX.Element {
  const tone = link.state?.tone ?? "neutral";
  return (
    <>
      <span
        className="doc-heading-link__label"
        data-lead={link.name === undefined ? undefined : ""}
      >
        <Described label={link.label} description={link.description} />
      </span>
      {link.name !== undefined && (
        <>
          {" "}
          <span className="doc-heading-link__name">
            <Untrusted text={link.name} />
          </span>
        </>
      )}
      {link.state && (
        <>
          {" "}
          <span className="doc-heading-link__state" data-tone={tone}>
            {link.state.icon && (
              <span aria-hidden="true" className={cn("doc-heading-link__glyph", CHIP_ICON[tone])}>
                {link.state.icon}
              </span>
            )}
            <Untrusted text={link.state.label} />
          </span>
        </>
      )}
    </>
  );
}

/**
 * A chip's links, each a small chip of its own (`.doc-heading-link`): a link where it has a
 * target, plain and dashed where it has none (a section the plan names that does not exist).
 */
function ChipLinks({ chip }: { chip: HeadingChip }): JSX.Element {
  return (
    <span className="doc-heading-links">
      {chip.links?.map((link, index) => (
        <span key={`${index}:${link.label}`} className="doc-heading-links__item">
          {index > 0 && " "}
          {link.to === undefined ? (
            <span className="doc-heading-link" title={hoverTitle(link.title)}>
              <LinkWords link={link} />
            </span>
          ) : (
            <Link
              to={link.to}
              title={hoverTitle(link.title)}
              className={cn("doc-heading-link", FOCUS)}
            >
              <LinkWords link={link} />
            </Link>
          )}
        </span>
      ))}
    </span>
  );
}

/** The chip's words: its label, then its link chips. */
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
 * given, each row its status (or the control that stands for it), then its detail line under
 * the words and its action at the end.
 */
export function StatusCard({ chips }: { chips: readonly HeadingChip[] }): JSX.Element {
  return (
    <ul className="doc-heading-status plan-surface" aria-label="Status">
      {chips.map((chip) => (
        <li key={chip.key} className="doc-heading-row" data-action={chip.action ? "" : undefined}>
          {chip.control ?? <Chip chip={chip} />}
          {chip.detail && <div className="doc-heading-row__detail plan-fine">{chip.detail}</div>}
          {chip.action && <div className="doc-heading-row__action">{chip.action}</div>}
        </li>
      ))}
    </ul>
  );
}

/**
 * The facts (`.doc-heading-facts`): what the document relates to, one labelled row per relation
 * ("Implements" with a chip per section, "Built by" with a chip per plan and its state). A row
 * keeps the chip's tooltip and tone attributes, so its text and links read as the chip's do.
 */
export function Facts({ chips }: { chips: readonly HeadingChip[] }): JSX.Element {
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
          <dt className="plan-eyebrow">
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
