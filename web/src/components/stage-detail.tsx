import { cn } from "cn";
import type { ReactNode } from "react";

import { formatElapsed } from "@/lib/format";

/// A titled group of fields on the stage page.
export function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section
      aria-label={title}
      className="flex flex-col gap-2 rounded-lg border border-hairline bg-card p-4"
    >
      <h2 className="eyebrow">{title}</h2>
      <dl className="grid grid-cols-[minmax(7rem,auto)_1fr] gap-x-4 gap-y-1.5 text-sm">
        {children}
      </dl>
    </section>
  );
}

/// One label/value pair. `null`/`undefined` renders a dimmed dash so absent
/// data still has a row.
export function Field({
  label,
  children,
  mono = false,
}: {
  label: string;
  children: ReactNode;
  mono?: boolean;
}) {
  const empty = children === null || children === undefined || children === "";
  return (
    <>
      <dt className="text-muted-foreground">{label}</dt>
      <dd
        className={cn(
          "min-w-0 break-words",
          mono && "font-mono text-xs",
          empty && "text-muted-foreground/60",
        )}
      >
        {empty ? "—" : children}
      </dd>
    </>
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
