import { Link } from "react-router";

import type { Attention, FailingCriterion } from "@/api/schema";
import { terminalHref } from "@/components/terminal/terminal-view";

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
              <span className="text-xs wrap-anywhere text-muted-foreground">
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
/// failing criteria and the way to its last terminal output. Disputes already
/// ruled on are history, shown in the stage's history card. Renders nothing for
/// an entry with neither.
export function BlockedFacts({ entry }: { entry: Attention }) {
  return (
    <>
      {entry.failing_criteria.length > 0 && <FailingCriteria criteria={entry.failing_criteria} />}
      {entry.terminal_history && (
        <Link to={terminalHref(entry.id)} className="w-fit text-xs font-medium hover:underline">
          View terminal history
        </Link>
      )}
    </>
  );
}
