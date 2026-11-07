import { cn } from "cn";
import { ChevronRight, Copy, File as FileIcon, FileText, Sparkles } from "lucide-react";
import { useEffect, useRef, useState, type JSX, type ReactNode } from "react";
import { Link } from "react-router";

import { Untrusted } from "@/plan/chat/visible-controls";
import { CopyButton, selectContents } from "@/plan/markdown/copy-button";

import "./doc-heading.css";
import { FactsLine, FOCUS, StatusCard } from "./doc-heading-parts";
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
  /** The control that acts on this status, set at its row's end: the summary's Rebuild. */
  action?: ReactNode;
  /** One fine line under the words: the approval sentence a rebuild waits on. */
  detail?: ReactNode;
  /**
   * `status` (the default) is a row of the status card: what needs attention. `fact` is a pair
   * on the facts line under it: what the document relates to (Built by, Implements).
   */
  kind?: "status" | "fact";
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
 * Props of a control in the heading's toolbar (Summary, Source, Edit, Discuss, Open all): the
 * quiet look in the row size, drawn by `.plan-button` alone (DESIGN.md, Buttons). Spread onto a
 * plain `<button>`, a `Link` or a `span`, never onto the shared `Button`, whose own height,
 * padding and corner utilities would put a second scale beside `.plan-button`'s.
 */
export const HEADING_BUTTON = {
  "data-size": "sm",
  "data-look": "quiet",
  className: "plan-button inline-flex shrink-0 items-center justify-center",
} as const;

/**
 * One group of the toolbar bar (`.doc-heading-group`, `doc-heading.css`): a segment of the bar,
 * divided from the next by a rule. A group whose children all render nothing is hidden by CSS
 * (`:empty`), and its rule goes with it.
 */
export function ToolbarGroup({ children }: { children: ReactNode }): JSX.Element {
  return <span className="doc-heading-group">{children}</span>;
}

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

/** The tooltip and description of a segment that has nowhere to go, as Edit's lock gives its reason. */
const VIEW_UNAVAILABLE = {
  summary: "This document has no summary yet",
  source: "This page has no source document",
} as const;

/**
 * Summary and Source: one two-state segmented control, the toolbar bar's first group, with the
 * current view filled (`.doc-heading-views` in `doc-heading.css`). Both states stay on screen,
 * so the way back from the source is always the segment beside the one the reader is on.
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
            title={current ? undefined : VIEW_UNAVAILABLE[view]}
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

/** The mark and the title, sharing one 30px line band (`doc-heading.css`). */
function TitleRow({ kind, title, rebuilding }: DocHeadingProps): JSX.Element {
  return (
    <div className="doc-heading-title-row">
      {kind !== null && <DocMark kind={kind} size="lg" rebuilding={rebuilding} />}
      <h1 className="plan-page-title min-w-0 font-[650] text-balance [overflow-wrap:anywhere]">
        {typeof title === "string" ? <Untrusted text={title} /> : title}
      </h1>
    </div>
  );
}

/**
 * The toolbar bar under the title: the view switch in the first group, then the caller's
 * actions, each in its own `ToolbarGroup`. Absent with nothing to hold.
 */
function Toolbar({ views, actions }: DocHeadingProps): JSX.Element | null {
  if (!views && actions == null) return null;
  return (
    <div className="doc-heading-toolbar">
      {views && (
        <ToolbarGroup>
          <ViewSwitch views={views} />
        </ToolbarGroup>
      )}
      {actions}
    </div>
  );
}

export function DocHeading(props: DocHeadingProps): JSX.Element {
  const { breadcrumb, chips = [], path, changes, children } = props;
  const status = chips.filter((chip) => chip.kind !== "fact");
  const facts = chips.filter((chip) => chip.kind === "fact");
  return (
    <header className="doc-heading">
      <Breadcrumbs crumbs={breadcrumb} />
      <TitleRow {...props} />
      <Toolbar {...props} />
      {status.length > 0 && <StatusCard chips={status} />}
      {facts.length > 0 && <FactsLine chips={facts} />}
      {path && <PathRow key={path} path={path} changes={changes} />}
      {children}
    </header>
  );
}
