import { useState } from "react";
import { Link } from "react-router";

import type { DocView, PlanView } from "@/plan/api/schema-files";
import { toAddress } from "@/plan/files/address";
import { PlanActionsExtras } from "@/plan/files/plan/plan-actions-extras";
import { ValidationList, warningCount } from "@/plan/files/plan/validation-list";

function plural(count: number, word: string): string {
  return `${count} ${word}${count === 1 ? "" : "s"}`;
}

function Implements({ plan }: { plan: PlanView }) {
  if (plan.implements.length === 0) return null;
  return (
    <div className="text-[13px]">
      <span className="text-muted-foreground">Implements: </span>
      {plan.implements.map((link, index) => (
        <span key={link.number}>
          {index > 0 && ", "}
          {link.path === null ? (
            <s title="not found" className="text-muted-foreground">
              Section {link.number}
            </s>
          ) : (
            <Link to={toAddress(link.path)} className="underline underline-offset-2">
              Section {link.number}
            </Link>
          )}
        </span>
      ))}
    </div>
  );
}

function ValidationSummary({ plan }: { plan: PlanView }) {
  const [open, setOpen] = useState(false);
  const validation = plan.validation;
  if (validation === null) return null;
  const errors = validation.errors.length;
  return (
    <div>
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        className={`text-[13px] font-medium underline-offset-2 hover:underline ${
          errors > 0 ? "text-destructive" : "text-muted-foreground"
        }`}
      >
        {plural(errors, "error")}, {plural(warningCount(validation), "warning")}
      </button>
      {open && (
        <div className="mt-2">
          <ValidationList validation={validation} />
        </div>
      )}
    </div>
  );
}

/// State, lock, the product sections the plan implements, its dates and the
/// validation result.
export function PlanHeader({ doc, plan }: { doc: DocView; plan: PlanView }) {
  return (
    <header className="mb-4 space-y-1.5 border-b border-(--hairline) pb-3">
      <div className="flex flex-wrap items-baseline gap-x-4 gap-y-1 text-[13px]">
        <span className="font-medium">{plan.state}</span>
        {plan.pressure_date !== null && (
          <span className="text-muted-foreground">Pressure tested {plan.pressure_date}</span>
        )}
        {plan.last_change !== null && (
          <span className="text-muted-foreground">Last change {plan.last_change}</span>
        )}
      </div>
      {doc.lock !== null && (
        <p role="note" className="text-[13px] text-muted-foreground">
          {doc.lock.notice}
        </p>
      )}
      <Implements plan={plan} />
      <ValidationSummary plan={plan} />
      <PlanActionsExtras plan={plan} path={doc.path} />
    </header>
  );
}
