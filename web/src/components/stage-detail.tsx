import { cn } from "cn";
import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";

import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { formatElapsed } from "@/lib/format";

/// One label/value pair; the hint explains the field on hover.
export interface Row {
  label: string;
  hint: string;
  value: ReactNode;
  mono?: boolean;
}

export interface SectionSpec {
  title: string;
  icon: LucideIcon;
  rows: Row[];
}

/// A row, or nothing when the value is absent, so a section shows only what
/// the stage actually has.
export function row(label: string, hint: string, value: ReactNode, mono = false): Row | null {
  const absent =
    value === null ||
    value === undefined ||
    value === "" ||
    (Array.isArray(value) && value.length === 0);
  return absent ? null : { label, hint, value, mono };
}

export function present(rows: (Row | null)[]): Row[] {
  return rows.filter((entry): entry is Row => entry !== null);
}

/// A titled group of rows on the stage page and in the stage dialog.
export function Section({ title, icon, rows }: SectionSpec) {
  return (
    <LedgerCard title={title} icon={icon} className="break-inside-avoid">
      <Rows rows={rows} className="px-4 py-1" />
    </LedgerCard>
  );
}

/// The card every stage section sits in: an icon and title in a header
/// strip over the body. `aside` sits at the strip's right end.
export function LedgerCard({
  title,
  icon: Icon,
  aside,
  label = title,
  className,
  children,
}: {
  title: string;
  icon: LucideIcon;
  aside?: ReactNode;
  label?: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <section aria-label={label} className={cn("ledger-card", className)}>
      <header className="ledger-head">
        <span className="ledger-icon">
          <Icon aria-hidden="true" className="size-3" />
        </span>
        <h2 className="eyebrow">{title}</h2>
        {aside}
      </header>
      {children}
    </section>
  );
}

/// The label/value list on its own, for a section that brings its own frame.
export function Rows({ rows, className }: { rows: Row[]; className?: string }) {
  return (
    <dl className={cn("ledger-rows text-sm", className)}>
      {rows.map((entry) => (
        <div key={entry.label} className="ledger-row">
          <dt>
            <Hint label={entry.label} hint={entry.hint} />
          </dt>
          <dd className={cn("min-w-0 break-words", entry.mono && "font-mono text-xs")}>
            {entry.value}
          </dd>
        </div>
      ))}
    </dl>
  );
}

function Hint({ label, hint }: { label: string; hint: string }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          tabIndex={0}
          className="cursor-help text-muted-foreground underline decoration-transparent decoration-dotted underline-offset-4 transition-colors hover:decoration-muted-foreground/60 focus-visible:decoration-muted-foreground/60"
        >
          {label}
        </span>
      </TooltipTrigger>
      <TooltipContent side="left" className="max-w-xs text-left">
        {hint}
      </TooltipContent>
    </Tooltip>
  );
}

/// The stage's plan-authored summary, below its heading. Untrusted plain
/// text: rendered as a text node only, never `dangerouslySetInnerHTML`.
/// Capped in height with its own scroll so a long summary does not push the
/// rest of the card away. Renders nothing when the stage has none. The
/// agent-facing description is never shown.
export function StageSummaryText({ summary }: { summary: string | null }) {
  if (summary === null) return null;
  return (
    <p
      tabIndex={0}
      role="region"
      aria-label="stage summary"
      className="stage-summary max-h-24 max-w-[78ch] overflow-y-auto text-[13.5px] leading-relaxed whitespace-pre-line"
    >
      {summary}
    </p>
  );
}

export function yesNo(value: boolean): string {
  return value ? "yes" : "no";
}

export function secs(value: number | null): string | null {
  return value === null ? null : formatElapsed(value);
}

export function tokens(value: number | null): string | null {
  return value === null ? null : value.toLocaleString();
}
