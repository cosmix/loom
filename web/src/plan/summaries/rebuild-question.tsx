import { useAtomValue } from "jotai/react";

import type { SummaryQuestion } from "@/plan/api/schema-summaries";
import { Untrusted } from "@/plan/chat/visible-controls";
import { authorBlockReasonAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";

const basisText: Record<SummaryQuestion["basis"], string> = {
  last: "from the last builds",
  per_byte: "scaled from past builds",
  starting: "starting figure",
};

export function RebuildQuestion({
  question,
  canAnswer,
  onAnswer,
}: {
  question: SummaryQuestion;
  canAnswer: boolean;
  onAnswer: (rebuild: boolean) => void;
}) {
  const authorBlockReason = useAtomValue(authorBlockReasonAtom);
  const reason = canAnswer
    ? null
    : (authorBlockReason ?? "Only the browser holding authoring can answer");

  return (
    <section aria-label="Rebuild summaries" className="w-full min-w-0 max-w-full space-y-3 text-sm">
      <p>
        <span className="font-medium tabular-nums">{question.count}</span> summaries out of date.
        Rebuild all?
      </p>
      <ul
        aria-label="Out-of-date summaries"
        tabIndex={0}
        className="max-h-48 min-w-0 space-y-1 overflow-y-auto font-mono text-xs [overflow-wrap:anywhere]"
      >
        {question.ids.map((id) => (
          <li key={id}>
            <Untrusted text={id} />
          </li>
        ))}
      </ul>
      <p className="text-muted-foreground">
        Estimate:{" "}
        <span className="tabular-nums">
          {question.estimate_tokens.toLocaleString("en-US")} tokens
        </span>{" "}
        ({basisText[question.basis]}).
      </p>
      <div className="flex flex-wrap gap-3">
        <DisabledReason reason={reason} onClick={() => onAnswer(true)}>
          Rebuild all
        </DisabledReason>
        <DisabledReason reason={reason} onClick={() => onAnswer(false)}>
          Not now
        </DisabledReason>
      </div>
    </section>
  );
}
