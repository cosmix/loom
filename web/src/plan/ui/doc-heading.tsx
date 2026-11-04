import { cn } from "cn";
import { ChevronRight, Copy, File as FileIcon, FileText, Sparkles } from "lucide-react";
import { useEffect, useRef, useState, type JSX, type ReactNode } from "react";
import { Link } from "react-router";

import { Untrusted } from "@/plan/chat/visible-controls";
import { CopyButton, selectContents } from "@/plan/markdown/copy-button";

import "./doc-heading.css";
import { ChipRow, FOCUS } from "./doc-heading-parts";
import { DocMark } from "./doc-mark";
import type { DocKind } from "./glyphs";

export interface HeadingCrumb {
  label: string;
  to?: string;
  icon?: ReactNode;
  /** A plan's crumb is its slug, set in the mono face. */
  mono?: boolean;
}

export { CHIP_FILL, CHIP_ICON } from "./doc-heading-parts";

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
  /** An action that belongs to this chip, set after it on its baseline: the summary's Rebuild. */
  after?: ReactNode;
}

export interface HeadingViews {
  current: "summary" | "source";
  summaryTo: string | null;
  sourceTo: string | null;
}

export interface DocHeadingProps {
  breadcrumb: readonly HeadingCrumb[];
  kind: DocKind | null;
  /** A string is drawn as untrusted text; a caller whose title is Markdown phrasing passes the node. */
  title: ReactNode;
  rebuilding?: boolean;
  views?: HeadingViews;
  actions?: ReactNode;
  chips?: readonly HeadingChip[];
  path?: string | null;
  changes?: { to: string } | null;
  children?: ReactNode;
}

/**
 * Props of a control in the heading's action row (Summary, Source, Edit, Discuss, Open all): the
 * quiet look in the row size, drawn by `.plan-button` alone (DESIGN.md, Buttons). Spread onto a
 * plain `<button>`, a `Link` or a `span`, never onto the shared `Button`, whose own height,
 * padding and corner utilities would put a second scale beside `.plan-button`'s.
 */
export const HEADING_BUTTON = {
  "data-size": "sm",
  "data-look": "quiet",
  className: "plan-button inline-flex shrink-0 items-center justify-center",
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

/**
 * Summary and Source: one two-state segmented control, a hairline track the height of the row's
 * other controls with the current view filled (`.doc-heading-views` in `doc-heading.css`). Both
 * states stay on screen, so the way back from the source is always the segment beside the one
 * the reader is on.
 */
function ViewSwitch({ views }: { views: HeadingViews }): JSX.Element {
  return (
    <div role="group" aria-label="View" className="doc-heading-views">
      {(["summary", "source"] as const).map((view) => {
        const current = views.current === view;
        const to = view === "summary" ? views.summaryTo : views.sourceTo;
        const label = view === "summary" ? "Summary" : "Source";
        const Icon = view === "summary" ? Sparkles : FileText;
        const content = (
          <>
            <Icon aria-hidden="true" />
            {label}
          </>
        );
        return to === null ? (
          <span
            key={view}
            {...HEADING_BUTTON}
            aria-current={current ? "page" : undefined}
            aria-disabled={current ? undefined : true}
          >
            {content}
          </span>
        ) : (
          <Link key={view} to={to} {...HEADING_BUTTON} aria-current={current ? "page" : undefined}>
            {content}
          </Link>
        );
      })}
    </div>
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
    // not follow the tree and chat columns. Mark, title and actions share one 30px line band
    // (`doc-heading.css`), so the controls centre on the title's first line even when it wraps.
    <div className="doc-heading-title-row flex flex-wrap items-start gap-x-4 gap-y-2">
      <div className="flex min-w-0 flex-[1_1_max-content] items-start gap-3">
        {kind !== null && <DocMark kind={kind} size="lg" rebuilding={rebuilding} />}
        <h1 className="plan-page-title min-w-0 font-[650] text-balance [overflow-wrap:anywhere]">
          {typeof title === "string" ? <Untrusted text={title} /> : title}
        </h1>
      </div>
      <div className="doc-heading-actions ml-auto flex flex-wrap items-center justify-end gap-2 max-[900px]:basis-full max-[900px]:justify-start">
        {views && <ViewSwitch views={views} />}
        {actions}
      </div>
    </div>
  );
}

export function DocHeading(props: DocHeadingProps): JSX.Element {
  const { breadcrumb, chips, path, changes, children } = props;
  return (
    <header className="doc-heading grid gap-2.5 border-b border-hairline bg-background pt-[18px] pb-3.5 font-sans">
      <Breadcrumbs crumbs={breadcrumb} />
      <TitleRow {...props} />
      {chips && chips.length > 0 && <ChipRow chips={chips} />}
      {path && <PathRow key={path} path={path} changes={changes} />}
      {children}
    </header>
  );
}
