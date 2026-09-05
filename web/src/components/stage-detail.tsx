import { cn } from "cn";
import { Fragment, type ReactNode } from "react";

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
export function Section({ title, rows }: SectionSpec) {
  return (
    <section
      aria-label={title}
      className="flex flex-col gap-2 rounded-lg border border-hairline bg-card p-4"
    >
      <h2 className="eyebrow">{title}</h2>
      <Rows rows={rows} />
    </section>
  );
}

/// The label/value grid on its own, for a section that brings its own frame.
export function Rows({ rows }: { rows: Row[] }) {
  return (
    <dl className="grid grid-cols-[minmax(7rem,auto)_1fr] gap-x-4 gap-y-1.5 text-sm">
      {rows.map((entry) => (
        <Fragment key={entry.label}>
          <dt>
            <Hint label={entry.label} hint={entry.hint} />
          </dt>
          <dd className={cn("min-w-0 break-words", entry.mono && "font-mono text-xs")}>
            {entry.value}
          </dd>
        </Fragment>
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
          className="cursor-help text-muted-foreground underline decoration-muted-foreground/40 decoration-dotted underline-offset-4"
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

export function yesNo(value: boolean): string {
  return value ? "yes" : "no";
}

export function secs(value: number | null): string | null {
  return value === null ? null : formatElapsed(value);
}

export function tokens(value: number | null): string | null {
  return value === null ? null : value.toLocaleString();
}
