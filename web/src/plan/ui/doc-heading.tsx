import { cn } from "cn";
import { ChevronRight, Copy, File as FileIcon, FileText, Sparkles } from "lucide-react";
import { useEffect, useRef, useState, type JSX, type ReactNode } from "react";
import { Link } from "react-router";

import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { CopyButton, selectContents } from "@/plan/markdown/copy-button";

import "./doc-heading.css";
import { DocMark } from "./doc-mark";
import type { DocKind } from "./glyphs";

export interface HeadingCrumb {
  label: string;
  to?: string;
  icon?: ReactNode;
  /** A plan's crumb is its slug, set in the mono face. */
  mono?: boolean;
}

export type HeadingTone = "neutral" | "ok" | "warning" | "danger" | "accent";

export interface HeadingChipLink {
  label: string;
  /** Without a target the link renders plain, with no strike-through. */
  to?: string;
  title?: string;
  description?: string;
}

export interface HeadingChip {
  key: string;
  label: string;
  /** Inline links after the label: "Implements §05, §09". */
  links?: readonly HeadingChipLink[];
  linksSeparator?: string;
  icon?: ReactNode;
  tone?: HeadingTone;
  title?: string;
  description?: string;
  to?: string;
  onClick?: () => void;
  pressed?: boolean;
}

export interface HeadingViews {
  current: "summary" | "source";
  summaryTo: string | null;
  sourceTo: string | null;
}

export interface DocHeadingProps {
  breadcrumb: readonly HeadingCrumb[];
  kind: DocKind | null;
  title: string;
  rebuilding?: boolean;
  views?: HeadingViews;
  actions?: ReactNode;
  chips?: readonly HeadingChip[];
  path?: string | null;
  changes?: { to: string } | null;
  children?: ReactNode;
}

const FOCUS = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring";
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

/**
 * Props of a toolbar action in the heading (Source, Edit, Discuss, Open all): the quiet look in
 * the row size, drawn by `.plan-button` in `design.css` (DESIGN.md, Buttons). Spread onto a
 * `Button`; the stylesheet, not a wrapper, restyles it.
 */
export const HEADING_BUTTON = {
  variant: "ghost",
  size: "sm",
  "data-look": "quiet",
  className: "plan-button",
} as const;

function Breadcrumbs({ crumbs }: { crumbs: readonly HeadingCrumb[] }): JSX.Element {
  return (
    <nav aria-label="Breadcrumb" className="plan-fine">
      <ol className="flex flex-wrap items-center gap-1.5">
        {crumbs.map((crumb, index) => {
          const current = index === crumbs.length - 1 ? "page" : undefined;
          const content = (
            <>
              {crumb.icon && (
                <span aria-hidden="true" className="inline-flex shrink-0 [&_svg]:size-3.5">
                  {crumb.icon}
                </span>
              )}
              <Untrusted text={crumb.label} />
            </>
          );
          const className = cn("inline-flex items-center gap-1.5", crumb.mono && "font-mono");
          return (
            <li key={`${index}:${crumb.label}`} className="flex min-w-0 items-center gap-1.5">
              {index > 0 && <ChevronRight aria-hidden="true" className="size-3 shrink-0" />}
              {crumb.to !== undefined ? (
                <Link
                  to={crumb.to}
                  aria-current={current}
                  className={cn(className, FOCUS, "rounded hover:text-foreground")}
                >
                  {content}
                </Link>
              ) : (
                <span aria-current={current} className={className}>
                  {content}
                </span>
              )}
            </li>
          );
        })}
      </ol>
    </nav>
  );
}

/** A view link is a quiet button in the row size; `doc-heading.css` marks the current one. */
const VIEW_ITEM = "plan-button inline-flex items-center";

/** Summary and Source: two quiet actions side by side, the current one marked. */
function ViewSwitch({ views }: { views: HeadingViews }): JSX.Element {
  return (
    <div role="group" aria-label="View" className="inline-flex gap-0.5">
      {(["summary", "source"] as const).map((view) => {
        const current = views.current === view;
        const to = view === "summary" ? views.summaryTo : views.sourceTo;
        const label = view === "summary" ? "Summary" : "Source";
        const Icon = view === "summary" ? Sparkles : FileText;
        const look = { "data-look": "quiet", "data-size": "sm" } as const;
        const content = (
          <>
            <Icon aria-hidden="true" />
            {label}
          </>
        );
        return to === null ? (
          <span
            key={view}
            {...look}
            aria-current={current ? "page" : undefined}
            aria-disabled={current ? undefined : true}
            className={VIEW_ITEM}
          >
            {content}
          </span>
        ) : (
          <Link
            key={view}
            to={to}
            {...look}
            aria-current={current ? "page" : undefined}
            className={VIEW_ITEM}
          >
            {content}
          </Link>
        );
      })}
    </div>
  );
}

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
          className={cn("inline-flex shrink-0 [&_svg]:size-3.5", CHIP_ICON[chip.tone ?? "neutral"])}
        >
          {chip.icon}
        </span>
      )}
      <span className="min-w-0 [overflow-wrap:anywhere]">
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
  const className =
    "inline-flex min-h-6 max-w-full items-center gap-1.5 text-[13px] text-foreground";
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

/** Announcement text that clears itself after the copy button's own 1.5 s reset. */
function useTransientMessage() {
  const [message, setMessage] = useState("");
  useEffect(() => {
    if (!message) return;
    const id = window.setTimeout(() => setMessage(""), 1500);
    return () => window.clearTimeout(id);
  }, [message]);
  return [message, setMessage] as const;
}

function PathRow({
  path,
  changes,
}: {
  path: string;
  changes: DocHeadingProps["changes"];
}): JSX.Element {
  const [message, setMessage] = useTransientMessage();
  const code = useRef<HTMLElement>(null);
  return (
    <div className="plan-fine flex flex-wrap items-center gap-x-2 gap-y-1">
      <div className="flex min-w-0 items-center gap-2">
        <FileIcon aria-hidden="true" className="size-3.5 shrink-0" />
        <code ref={code} className="min-w-0 font-mono break-all">
          <Untrusted text={path} />
        </code>
        <CopyButton
          text={path}
          label="Copy path"
          icon={Copy}
          onCopied={() => setMessage("Copied")}
          onFailed={() => {
            selectContents(code.current);
            setMessage("Copy failed");
          }}
        />
      </div>
      {/* role="status" (implicitly polite and atomic), never an aria-live attribute: Radix
          modals hide the page through aria-hidden's hideOthers, which keeps every [aria-live]
          element and all its ancestors exposed, so the page behind an open drawer would stay
          reachable. */}
      <span role="status">{message}</span>
      {changes && (
        <span className="ml-auto inline-flex items-center gap-2">
          <span
            aria-hidden="true"
            className="size-[7px] shrink-0 rounded-full bg-(--tone-warning)"
          />
          <Link to={changes.to} className={cn(FOCUS, "rounded hover:text-foreground")}>
            Uncommitted changes
          </Link>
        </span>
      )}
    </div>
  );
}

function TitleRow({ kind, title, rebuilding, views, actions }: DocHeadingProps): JSX.Element {
  return (
    // The actions drop under the title exactly when its one-line width plus theirs do not fit,
    // whatever the column width; a grid track could not wrap, and the viewport breakpoint does
    // not follow the tree and chat columns.
    <div className="flex flex-wrap items-center gap-3">
      <div className="flex min-w-0 flex-[1_1_max-content] items-center gap-3">
        {kind !== null && <DocMark kind={kind} size="lg" rebuilding={rebuilding} />}
        <h1 className="plan-page-title min-w-0 font-[650] text-balance [overflow-wrap:anywhere]">
          <Untrusted text={title} />
        </h1>
      </div>
      <div className="ml-auto flex flex-wrap items-center justify-end gap-2 max-[900px]:basis-full max-[900px]:justify-start">
        {views && <ViewSwitch views={views} />}
        {actions}
      </div>
    </div>
  );
}

export function DocHeading(props: DocHeadingProps): JSX.Element {
  const { breadcrumb, chips, path, changes, children } = props;
  return (
    <header className="grid gap-2.5 border-b border-hairline bg-background px-[22px] pt-[18px] pb-3.5 font-sans">
      <Breadcrumbs crumbs={breadcrumb} />
      <TitleRow {...props} />
      {chips && chips.length > 0 && (
        <div className="flex flex-wrap items-center gap-x-4 gap-y-1">
          {chips.map((chip) => (
            <Chip key={chip.key} chip={chip} />
          ))}
        </div>
      )}
      {path && <PathRow key={path} path={path} changes={changes} />}
      {children}
    </header>
  );
}
