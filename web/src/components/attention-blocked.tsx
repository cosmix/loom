import { Link } from "react-router";

import type { Attention, DisputeLine, FailingCriterion } from "@/api/schema";
import { terminalHref } from "@/components/terminal/terminal-view";

const VERDICT_PAST: Record<string, string> = { accept: "accepted", reject: "rejected" };
const OP_PAST: Record<string, string> = {
  replace: "replaced",
  delete: "deleted",
  append: "added",
  insert: "added",
};

/// One dispute as a short phrase: the criterion edits its verdict made, else
/// what it contested and the ruling ("#2 criterion 12 replaced", "#1 integrity
/// accepted"). An unknown verb or ruling is shown as the record wrote it.
export function disputePhrase(dispute: DisputeLine): string {
  if (dispute.ops.length > 0) {
    const noun = dispute.field && dispute.field !== "acceptance" ? dispute.field : "criterion";
    const edits = dispute.ops.map((op) => {
      const done = OP_PAST[op.op] ?? op.op;
      return op.index === null ? `${noun} ${done}` : `${noun} ${op.index} ${done}`;
    });
    return `#${dispute.id} ${edits.join(", ")}`;
  }
  const ruling =
    dispute.verdict === null ? "pending" : (VERDICT_PAST[dispute.verdict] ?? dispute.verdict);
  return `#${dispute.id} ${dispute.kind} ${ruling}`;
}

/// The acceptance criteria that failed in the stage's last attempt, numbered
/// as `loom stage amend --index` counts them. Untrusted text: text nodes only.
function FailingCriteria({ criteria }: { criteria: FailingCriterion[] }) {
  return (
    <div className="flex flex-col gap-1">
      <h3 className="eyebrow">failing criteria</h3>
      <ul className="flex flex-col gap-1.5" aria-label="failing criteria">
        {criteria.map((criterion) => (
          <li key={criterion.index} className="flex flex-col gap-0.5">
            <span className="flex items-start gap-1.5">
              <span className="font-mono text-xs text-muted-foreground">#{criterion.index}</span>
              <code className="min-w-0 text-xs wrap-anywhere">{criterion.command}</code>
            </span>
            {criterion.diagnostic && (
              <span className="text-xs break-words text-muted-foreground">
                {criterion.diagnostic}
              </span>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}

/// What a blocked stage's card says about the block beyond its reason: the
/// failing criteria, the disputes already ruled on, and the way to its last
/// terminal output. Renders nothing for an entry with none of them.
export function BlockedFacts({ entry }: { entry: Attention }) {
  const { failing_criteria: failing, disputes } = entry;
  return (
    <>
      {failing.length > 0 && <FailingCriteria criteria={failing} />}
      {disputes.length > 0 && (
        <p className="text-xs break-words text-muted-foreground">
          Disputes: {disputes.map(disputePhrase).join(" · ")}
        </p>
      )}
      {entry.terminal_history && (
        <Link to={terminalHref(entry.id)} className="w-fit text-xs font-medium hover:underline">
          View terminal history
        </Link>
      )}
    </>
  );
}
