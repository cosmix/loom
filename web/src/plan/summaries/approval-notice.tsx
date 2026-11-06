import { useAtomValue } from "jotai/react";
import { useRef, useState } from "react";

import type { SummaryQuestion } from "@/plan/api/schema-summaries";
import { Untrusted } from "@/plan/chat/visible-controls";
import { canAuthorAtom } from "@/plan/state/atoms";
import { type ButtonLook, DisabledReason, INLINE_BASELINE } from "@/plan/ui/disabled-reason";

import { summaryQuestionAtom, useSummaryActions } from "./state";

/** The one sentence of a pending rebuild: the count, the reason and the estimate. */
export function ApprovalSentence({ question }: { question: SummaryQuestion }) {
  return (
    <>
      <span className="tabular-nums">{question.count}</span> summaries out of date. The rebuild is
      above the budget and waiting for approval:{" "}
      <span className="tabular-nums">
        {question.estimate_tokens.toLocaleString("en-US")} tokens
      </span>
      .
    </>
  );
}

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
        <ApprovalSentence question={question} />
      </span>
    </span>
  );
}

type Failure = { questionId: string; message: string };

/** The pending question, whether this browser may answer it, the answer, and the last failure. */
function useAnswerApproval() {
  const question = useAtomValue(summaryQuestionAtom);
  const canAnswer = useAtomValue(canAuthorAtom);
  const { answer } = useSummaryActions();
  const pending = useRef(false);
  const [failure, setFailure] = useState<Failure | null>(null);
  const onAnswer = (rebuild: boolean) => {
    if (!question || !canAnswer || pending.current) return;
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
  const shown = question !== null && failure?.questionId === question.id ? failure : null;
  return { question, canAnswer, onAnswer, failure: shown };
}

function FailureAlert({ failure }: { failure: Failure | null }) {
  if (failure === null) return null;
  return (
    <span role="alert" className="text-xs text-destructive">
      <Untrusted text={failure.message} />
    </span>
  );
}

export function PendingApprovalNotice() {
  const { question, canAnswer, onAnswer, failure } = useAnswerApproval();
  if (!question) return null;
  return (
    <span className={`${INLINE_BASELINE} min-w-0`}>
      <ApprovalNotice question={question} canAnswer={canAnswer} onAnswer={onAnswer} />
      <FailureAlert failure={failure} />
    </span>
  );
}

/**
 * The answer alone: "Rebuild all" for an author (nothing for a reader), with a failed answer's
 * message after it. The document heading's status card sets it at the status row's end and
 * draws the sentence (`ApprovalSentence`) as the row's detail.
 */
export function ApprovalAnswer({ look }: { look?: ButtonLook }) {
  const { question, canAnswer, onAnswer, failure } = useAnswerApproval();
  if (!question || !canAnswer) return null;
  return (
    <span role="group" aria-label="Rebuild approval" className={`${INLINE_BASELINE} min-w-0`}>
      <DisabledReason
        reason={null}
        size="sm"
        look={look}
        reasonPlacement="inline"
        onClick={() => onAnswer(true)}
      >
        Rebuild all
      </DisabledReason>
      <FailureAlert failure={failure} />
    </span>
  );
}
