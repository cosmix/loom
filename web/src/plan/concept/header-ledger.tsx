import { Lock } from "lucide-react";
import { useId, type JSX, type ReactNode } from "react";

import "./header-ledger.css";

/*
 * The ledger of an overview page's heading (ui/DESIGN.md, Page rhythm): one hairline-divided row
 * per status, each a label, the status with its detail, and the action at the row's end. The
 * product overview, the metaplan and the vision page draw their rows through these parts, so a
 * status reads the same everywhere. Tone is on the glyph alone.
 */

export type LedgerTone = "completed" | "warning" | "executing" | "pending" | "blocked";

/** The rows' container; it takes no space while no row renders. */
export function LedgerRows({ children }: { children: ReactNode }): JSX.Element {
  return <div className="header-ledger">{children}</div>;
}

/**
 * One row: a region named by its label, so a row's controls are found by the row they belong to.
 * `wide` gives the whole row after the label to its content (a list); otherwise the
 * action sits at the row's end, and a row without one draws no action column.
 */
export function LedgerRow({
  label,
  action,
  wide = false,
  children,
}: {
  label: string;
  action?: ReactNode;
  wide?: boolean;
  children: ReactNode;
}): JSX.Element {
  const labelId = useId();
  return (
    <section aria-labelledby={labelId} className="header-ledger__row" data-wide={wide || undefined}>
      <span id={labelId} className="header-ledger__label">
        {label}
      </span>{" "}
      <div className="header-ledger__main">{children}</div>
      {action != null && (
        <>
          {" "}
          <div className="header-ledger__action">{action}</div>
        </>
      )}
    </section>
  );
}

/** The status: a toned 16px glyph and its words at the ledger's status step. */
export function LedgerStatus({
  tone,
  glyph,
  role,
  children,
}: {
  tone?: LedgerTone;
  glyph: ReactNode;
  role?: "status";
  children: ReactNode;
}): JSX.Element {
  return (
    <div className="header-ledger__status" data-tone={tone} role={role}>
      <span className="header-ledger__glyph" aria-hidden="true">
        {glyph}
      </span>
      <span>{children}</span>
    </div>
  );
}

/** What the status means, under its words. */
export function LedgerDetail({ children }: { children: ReactNode }): JSX.Element {
  return <p className="header-ledger__detail">{children}</p>;
}

/**
 * Why the action beside a status is blocked: a muted line led by a lock, attached to the status.
 * The button names it as its description (`id`), so the reason is drawn once.
 */
export function BlockedReason({ id, children }: { id: string; children: ReactNode }): JSX.Element {
  return (
    <p id={id} className="header-ledger__detail header-ledger__reason">
      <Lock aria-hidden="true" />
      <span>{children}</span>
    </p>
  );
}

/** Counted work under the status: a 3px line that carries its count. */
export function LedgerProgress({
  done,
  total,
  label,
}: {
  done: number;
  total: number;
  label: string;
}): JSX.Element {
  const share = total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0;
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={total}
      aria-valuenow={done}
      className="header-ledger__progress"
    >
      <span style={{ width: `${share}%` }} />
    </div>
  );
}
