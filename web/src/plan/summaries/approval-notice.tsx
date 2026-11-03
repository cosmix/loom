import { useAtomValue } from "jotai/react";
import { useRef, useState } from "react";

import type { SummaryQuestion } from "@/plan/api/schema-summaries";
import { Untrusted } from "@/plan/chat/visible-controls";
import { canAuthorAtom } from "@/plan/state/atoms";
import { DisabledReason, INLINE_BASELINE } from "@/plan/ui/disabled-reason";

import { summaryQuestionAtom, useSummaryActions } from "./state";

/**
 * An above-budget rebuild waiting for approval, as one line beside the summary status: "Rebuild
 * all" for an author, then one fine-print sentence with the count, the reason and the estimate.
 * A reader sees the sentence alone.
 */
export function ApprovalNotice({
  question,
  canAnswer,
  onAnswer,
}: {
  question: SummaryQuestion;
  canAnswer: boolean;
  onAnswer: (rebuild: boolean) => void;
}) {
  return (
    <span role="group" aria-label="Rebuild approval" className={`${INLINE_BASELINE} min-w-0`}>
      {canAnswer && (
        <DisabledReason
          reason={null}
          size="sm"
          reasonPlacement="inline"
          onClick={() => onAnswer(true)}
        >
          Rebuild all
        </DisabledReason>
      )}
      <span className="plan-fine">
        <span className="tabular-nums">{question.count}</span> summaries out of date. The rebuild is
        above the budget and waiting for approval:{" "}
        <span className="tabular-nums">
          {question.estimate_tokens.toLocaleString("en-US")} tokens
        </span>
        .
      </span>
    </span>
  );
}

export function PendingApprovalNotice() {
  const question = useAtomValue(summaryQuestionAtom);
  const canAnswer = useAtomValue(canAuthorAtom);
  const { answer } = useSummaryActions();
  const pending = useRef(false);
  const [failure, setFailure] = useState<{ questionId: string; message: string } | null>(null);

  if (!question) return null;

  const onAnswer = (rebuild: boolean) => {
    if (!canAnswer || pending.current) return;
    pending.current = true;
    setFailure(null);
    void answer(question.id, rebuild)
      .then((message) => {
        setFailure(message === null ? null : { questionId: question.id, message });
      })
      .finally(() => {
        pending.current = false;
      });
  };

  return (
    <span className={`${INLINE_BASELINE} min-w-0`}>
      <ApprovalNotice question={question} canAnswer={canAnswer} onAnswer={onAnswer} />
      {failure?.questionId === question.id && (
        <span role="alert" className="text-xs text-destructive">
          <Untrusted text={failure.message} />
        </span>
      )}
    </span>
  );
}
