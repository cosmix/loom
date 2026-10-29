import { cn } from "cn";
import { ChevronRight, Copy } from "lucide-react";
import { useEffect, useRef, useState, type JSX, type ReactNode } from "react";
import { Link } from "react-router";

import { Untrusted } from "@/plan/chat/visible-controls";
import { CopyButton, selectContents } from "@/plan/markdown/copy-button";

import { DocMark } from "./doc-mark";
import { OTHER_GLYPHS, type DocKind } from "./glyphs";

export interface HeadingCrumb {
  label: string;
  to?: string;
}

export type HeadingTone = "neutral" | "ok" | "warning" | "danger" | "accent";

export interface HeadingChip {
  key: string;
  label: string;
  icon?: ReactNode;
  tone?: HeadingTone;
  title?: string;
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
  lock?: string | null;
  children?: ReactNode;
}

const FOCUS = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring";
const CHIP_TONE: Record<HeadingTone, string> = {
  neutral: "text-muted-foreground",
  ok: "text-(--tone-completed)",
  warning: "text-(--tone-warning)",
  danger: "text-(--tone-blocked)",
  accent: "text-(--tone-executing)",
};
const ReadOnlyGlyph = OTHER_GLYPHS.readOnly;

function Breadcrumbs({ crumbs }: { crumbs: readonly HeadingCrumb[] }): JSX.Element {
  return (
    <nav aria-label="Breadcrumb" className="text-xs text-muted-foreground">
      <ol className="flex flex-wrap items-center gap-1.5">
        {crumbs.map((crumb, index) => {
          const current = index === crumbs.length - 1 ? "page" : undefined;
          return (
            <li key={`${index}:${crumb.label}`} className="flex min-w-0 items-center gap-1.5">
              {index > 0 && <ChevronRight aria-hidden="true" className="size-3 shrink-0" />}
              {crumb.to !== undefined ? (
                <Link
                  to={crumb.to}
                  aria-current={current}
                  className={cn(FOCUS, "rounded hover:text-foreground")}
                >
                  <Untrusted text={crumb.label} />
                </Link>
              ) : (
                <span aria-current={current}>
                  <Untrusted text={crumb.label} />
                </span>
              )}
            </li>
          );
        })}
      </ol>
    </nav>
  );
}

function ViewSwitch({ views }: { views: HeadingViews }): JSX.Element {
  return (
    <div
      role="group"
      aria-label="View"
      className="inline-flex rounded-md border border-border p-0.5 text-xs font-medium"
    >
      {(["summary", "source"] as const).map((view) => {
        const current = views.current === view;
        const to = view === "summary" ? views.summaryTo : views.sourceTo;
        const label = view === "summary" ? "Summary" : "Source";
        const className = cn(
          "inline-flex min-h-6 items-center rounded px-2 py-1",
          current ? "bg-muted text-foreground" : "text-muted-foreground",
        );
        return to === null ? (
          <span
            key={view}
            aria-current={current ? "page" : undefined}
            aria-disabled={current ? undefined : true}
            className={cn(className, !current && "opacity-50")}
          >
            {label}
          </span>
        ) : (
          <Link
            key={view}
            to={to}
            aria-current={current ? "page" : undefined}
            className={cn(className, FOCUS, "hover:bg-muted")}
          >
            {label}
          </Link>
        );
      })}
    </div>
  );
}

function Chip({ chip }: { chip: HeadingChip }): JSX.Element {
  const className = cn(
    "inline-flex min-h-6 max-w-full items-center gap-1.5 rounded-full border border-border px-2 py-0.5 text-xs",
    CHIP_TONE[chip.tone ?? "neutral"],
  );
  const content = (
    <>
      {chip.icon && (
        <span aria-hidden="true" className="inline-flex shrink-0 [&_svg]:size-3">
          {chip.icon}
        </span>
      )}
      <span className="min-w-0 [overflow-wrap:anywhere]">
        <Untrusted text={chip.label} />
      </span>
    </>
  );
  if (chip.to !== undefined) {
    return (
      <Link to={chip.to} title={chip.title} className={cn(className, FOCUS, "hover:bg-muted")}>
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
        title={chip.title}
        className={cn(className, FOCUS, "hover:bg-muted")}
      >
        {content}
      </button>
    );
  }
  return (
    <span title={chip.title} className={className}>
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
    <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
      <code ref={code} className="min-w-0 break-all font-mono">
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
      {/* role="status" (implicitly polite and atomic), never an aria-live attribute: Radix
          modals hide the page through aria-hidden's hideOthers, which keeps every [aria-live]
          element and all its ancestors exposed, so the page behind an open drawer would stay
          reachable. */}
      <span role="status">{message}</span>
      {changes && (
        <Link to={changes.to} className={cn(FOCUS, "rounded hover:text-foreground")}>
          Uncommitted changes
        </Link>
      )}
    </div>
  );
}

function TitleRow({ kind, title, rebuilding, views, actions }: DocHeadingProps): JSX.Element {
  return (
    <div
      className={cn(
        "grid items-center gap-3",
        kind === null ? "grid-cols-[minmax(0,1fr)_auto]" : "grid-cols-[auto_minmax(0,1fr)_auto]",
      )}
    >
      {kind !== null && <DocMark kind={kind} size="lg" rebuilding={rebuilding} />}
      <h1 className="font-display text-2xl [overflow-wrap:anywhere]">
        <Untrusted text={title} />
      </h1>
      <div className="flex flex-wrap items-center justify-end gap-2">
        {views && <ViewSwitch views={views} />}
        {actions}
      </div>
    </div>
  );
}

export function DocHeading(props: DocHeadingProps): JSX.Element {
  const { breadcrumb, chips, path, changes, lock, children } = props;
  return (
    <header className="flex flex-col gap-3 px-6 pt-5 pb-3 font-sans">
      <Breadcrumbs crumbs={breadcrumb} />
      <TitleRow {...props} />
      {chips && chips.length > 0 && (
        <div className="flex flex-wrap items-center gap-2">
          {chips.map((chip) => (
            <Chip key={chip.key} chip={chip} />
          ))}
        </div>
      )}
      {path && <PathRow key={path} path={path} changes={changes} />}
      {lock && (
        <p role="note" className="flex items-start gap-1.5 text-xs text-muted-foreground">
          <ReadOnlyGlyph aria-hidden="true" className="mt-0.5 size-3 shrink-0" />
          <Untrusted text={lock} />
        </p>
      )}
      {children}
    </header>
  );
}
