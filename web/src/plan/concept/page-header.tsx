import { Link } from "react-router";

import type { ConceptPage, OutOfDate, PlanLink } from "@/plan/api/schema-concept";
import { LockNotice } from "@/plan/files/lock-notice";
import { toAddress } from "@/plan/files/address";

import { PageHeaderExtras } from "./page-header-extras";

/** The reason a summary is out of date, in words (marks, chips and the header share it). */
export function outOfDateText(outOfDate: OutOfDate): string {
  switch (outOfDate.reason) {
    case "source_changed":
      return "Out of date: the source changed";
    case "rebuilding":
      return "Out of date: rebuilding";
    case "awaiting_approval":
      return "Out of date: waiting for approval";
    case "cannot_rebuild":
      return `Out of date: cannot rebuild${outOfDate.detail ? `: ${outOfDate.detail}` : ""}`;
    case "failed":
      return "Out of date: build failed";
  }
}

export function planStateText(state: string): string {
  return state.replaceAll("_", " ");
}

function summaryStatus(state: ConceptPage["state"]): string | null {
  if (!state) return null;
  if (state.out_of_date) return outOfDateText(state.out_of_date);
  switch (state.state) {
    case "current":
      return "Up to date";
    case "incomplete":
      return "Summary incomplete";
    case "rebuilding":
      return "Rebuilding";
    case "missing":
      return "No summary yet";
    case "stale":
      return "Out of date";
  }
}

const LOCKED_NOTICES: Record<string, { reason: "running" | "done"; notice: string }> = {
  running: {
    reason: "running",
    notice: "This plan is running. `loom run` works from the committed copy.",
  },
  done: { reason: "done", notice: "This plan is done. Record changes in a new plan." },
  retired: { reason: "done", notice: "This plan is done. Record changes in a new plan." },
};

function Implements({ page }: { page: ConceptPage }) {
  const view = page.implements;
  if (!view) return null;
  if (view.kind === "not_stated") return <p className="text-[13px]">Implements: not stated</p>;
  return (
    <p className="flex flex-wrap items-baseline gap-x-2 text-[13px]">
      <span>Implements:</span>
      {view.links.map((link) =>
        link.doc ? (
          <Link key={link.name} to={`/${link.doc.page}`} className="underline">
            {link.doc.title}
          </Link>
        ) : (
          <span key={link.name} className="font-mono text-muted-foreground">
            {link.name}
          </span>
        ),
      )}
    </p>
  );
}

function ImplementedBy({ plans }: { plans: PlanLink[] }) {
  if (plans.length === 0) return null;
  return (
    <p className="flex flex-wrap items-baseline gap-x-3 text-[13px]">
      <span>Implemented by:</span>
      {plans.map((plan) => (
        <span key={plan.slug}>
          <Link to={`/engineering/${encodeURIComponent(plan.slug)}`} className="underline">
            {plan.title}
          </Link>{" "}
          <span className="text-muted-foreground">({planStateText(plan.state)})</span>
        </span>
      ))}
    </p>
  );
}

function PlanFacts({ page }: { page: ConceptPage }) {
  if (page.plan_state === null) return null;
  const lock = LOCKED_NOTICES[page.plan_state];
  return (
    <>
      <p className="text-[13px] text-muted-foreground">
        Plan state: {planStateText(page.plan_state)}
        {page.last_pressure ? ` · Last pressure test ${page.last_pressure}` : ""}
      </p>
      {lock && <LockNotice lock={{ ...lock, plan: null }} />}
    </>
  );
}

/** A document summary page's header: title, path, colour, state, file mode link, cross links. */
export function PageHeader({ page }: { page: ConceptPage }) {
  const status = summaryStatus(page.state);
  return (
    <header className="flex flex-col gap-1.5 pb-4">
      <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
        {page.colour !== null && (
          <span
            aria-hidden="true"
            data-colour={page.colour}
            className="inline-block size-3 self-center rounded-sm"
            style={{ background: `var(--doc-${page.colour})` }}
          />
        )}
        <h1 className="font-display text-2xl">{page.title}</h1>
        {page.path && <span className="font-mono text-xs text-muted-foreground">{page.path}</span>}
      </div>
      <div className="flex flex-wrap items-baseline gap-x-3 text-[13px]">
        {status && <span data-dashed={page.state?.out_of_date ? "true" : undefined}>{status}</span>}
        {page.path && (
          <Link to={toAddress(page.path)} className="underline">
            Open in file mode
          </Link>
        )}
      </div>
      <PlanFacts page={page} />
      <Implements page={page} />
      <ImplementedBy plans={page.implemented_by} />
      <PageHeaderExtras page={page} />
    </header>
  );
}
