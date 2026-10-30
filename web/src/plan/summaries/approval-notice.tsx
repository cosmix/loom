import { useAtomValue } from "jotai/react";
import { useRef, useState } from "react";

import type { SummaryQuestion } from "@/plan/api/schema-summaries";
import { Untrusted } from "@/plan/chat/visible-controls";
import { canAuthorAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";

import { summaryQuestionAtom, useSummaryActions } from "./state";

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
    <section aria-label="Rebuild approval" className="w-full min-w-0 space-y-2 text-sm">
      <p>
        <span className="font-medium tabular-nums">{question.count}</span> summaries out of date.
        The rebuild is above the budget and waiting for approval.
      </p>
      <p className="text-muted-foreground">
        Estimate:{" "}
        <span className="tabular-nums">
          {question.estimate_tokens.toLocaleString("en-US")} tokens
        </span>
        .
      </p>
      {canAnswer && (
        <DisabledReason reason={null} onClick={() => onAnswer(true)}>
          Rebuild all
        </DisabledReason>
      )}
    </section>
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
    <div className="w-full min-w-0 space-y-2">
      <ApprovalNotice question={question} canAnswer={canAnswer} onAnswer={onAnswer} />
      {failure?.questionId === question.id && (
        <p role="alert" className="text-sm text-destructive">
          <Untrusted text={failure.message} />
        </p>
      )}
    </div>
  );
}
