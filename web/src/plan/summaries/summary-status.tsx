import { useAtomValue } from "jotai/react";
import { ChevronDown, ChevronUp, RefreshCwIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { useNow } from "@/components/hooks/use-now";
import { Button } from "@/components/ui/button";
import type { SummaryPanel as Panel, SummaryQuestion } from "@/plan/api/schema-summaries";
import "@/plan/header/activity.css";
import { Popover, PopoverContent, PopoverTrigger } from "@/plan/header/popover";
import { canAuthorAtom } from "@/plan/state/atoms";
import { ActivityTile } from "@/plan/ui/activity-glyphs";
import { LoadState } from "@/plan/ui/load-state";

import { RebuildQuestion } from "./rebuild-question";
import { summaryQuestionAtom, useSummaryActions, useSummaryFeed, type PanelLoad } from "./state";
import { SummaryPanel } from "./summary-panel";

function staleCount(panel: Panel): number {
  const questionIds = new Set(panel.question?.ids ?? []);
  const ids = new Set([...panel.queue, ...panel.failed.map(({ id }) => id), ...questionIds]);
  // Count each ID once across lists, retaining any question count beyond its listed IDs.
  return ids.size + Math.max(0, (panel.question?.count ?? 0) - questionIds.size);
}

function statusText(load: PanelLoad): { label: string; count: string } {
  if (load.status === "loading") return { label: "Loading summaries", count: "" };
  if (load.status === "error") return { label: "Summary rebuild status unavailable", count: "" };
  const { running } = load.panel;
  if (running) {
    return {
      label: `Rebuilding ${running.done} of ${running.total}`,
      count: `${running.done}/${running.total}`,
    };
  }
  const count = staleCount(load.panel);
  return count > 0
    ? { label: `${count} out of date`, count: String(count) }
    : { label: "Summaries up to date", count: "" };
}

function StatusTrigger({ compact, load }: { compact: boolean; load: PanelLoad }) {
  const { label, count } = statusText(load);
  return (
    <PopoverTrigger asChild>
      <Button type="button" variant="ghost" size="sm" aria-label={`Summary rebuilds: ${label}`}>
        {compact ? (
          <>
            <RefreshCwIcon aria-hidden="true" />
            {count && <span className="tabular-nums">{count}</span>}
          </>
        ) : (
          label
        )}
      </Button>
    </PopoverTrigger>
  );
}

function QuestionAnswer({ question }: { question: SummaryQuestion }) {
  const canAnswer = useAtomValue(canAuthorAtom);
  const { answer } = useSummaryActions();
  const pending = useRef(false);
  const [error, setError] = useState<string | null>(null);

  async function handleAnswer(rebuild: boolean) {
    if (pending.current) return;
    pending.current = true;
    setError(null);
    try {
      setError(await answer(question.id, rebuild));
    } finally {
      pending.current = false;
    }
  }

  return (
    <>
      <RebuildQuestion
        question={question}
        canAnswer={canAnswer}
        onAnswer={(rebuild) => void handleAnswer(rebuild)}
      />
      {error && (
        <p role="alert" className="text-destructive">
          {error}
        </p>
      )}
    </>
  );
}

function StatusContent({ load, reload }: { load: PanelLoad; reload: () => void }) {
  const nowSecs = Math.floor(useNow() / 1000);
  if (load.status === "loading") return <LoadState state="loading" title="Loading summaries" />;
  if (load.status === "error") {
    return (
      <LoadState
        state="error"
        message={load.message}
        action={{ label: "Retry", onClick: reload }}
      />
    );
  }
  return (
    <>
      {/* The question comes first so its answer buttons stay above the fold on a phone. */}
      {load.panel.question && (
        <QuestionAnswer key={load.panel.question.id} question={load.panel.question} />
      )}
      <SummaryPanel panel={load.panel} nowSecs={nowSecs} />
    </>
  );
}

/**
 * Open state for the popover. An arriving question opens it without taking focus and without
 * pulling focus to the trigger when it closes; an open the user starts behaves like any popover.
 */
export function useQuestionPopover(question: SummaryQuestion | null) {
  const [open, setOpen] = useState(false);
  const lastAnnouncedId = useRef<string | null>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const autoOpened = useRef(false);
  const closedFromOutside = useRef(false);

  useEffect(() => {
    if (!question || question.id === lastAnnouncedId.current) return;
    lastAnnouncedId.current = question.id;
    autoOpened.current = true;
    setOpen(true);
  }, [question]);

  return {
    open,
    contentRef,
    onOpenChange(next: boolean) {
      // Radix focuses the trigger on close, which would pull focus out of the composer when
      // Escape is pressed there; remember where focus was before the close moves anything.
      closedFromOutside.current =
        !next && autoOpened.current && !contentRef.current?.contains(document.activeElement);
      autoOpened.current = false;
      setOpen(next);
    },
    onOpenAutoFocus(event: Event) {
      if (autoOpened.current) event.preventDefault();
    },
    onCloseAutoFocus(event: Event) {
      if (closedFromOutside.current) event.preventDefault();
    },
  };
}

export function SummaryStatus({ compact = false }: { compact?: boolean }) {
  const { load, reload } = useSummaryFeed();
  const question = useAtomValue(summaryQuestionAtom);
  const popover = useQuestionPopover(question);

  return (
    <>
      <Popover open={popover.open} onOpenChange={popover.onOpenChange}>
        <StatusTrigger compact={compact} load={load} />
        <PopoverContent
          ref={popover.contentRef}
          aria-label="Summary rebuilds"
          collisionPadding={16}
          className="max-h-[80vh] w-[calc(100vw-2rem)] min-w-0 overflow-x-hidden overflow-y-auto [overflow-wrap:anywhere] sm:w-96"
          onOpenAutoFocus={popover.onOpenAutoFocus}
          onCloseAutoFocus={popover.onCloseAutoFocus}
        >
          <StatusContent load={load} reload={reload} />
        </PopoverContent>
      </Popover>
      <QuestionAnnouncement question={question} />
    </>
  );
}

/** Lives outside the activity popover, which mounts its rows only while open. */
export function QuestionAnnouncement({ question }: { question: SummaryQuestion | null }) {
  return (
    <div role="status" aria-live="polite" className="sr-only">
      {question &&
        `Rebuild ${question.count} summaries? Estimated ${question.estimate_tokens.toLocaleString("en-US")} tokens.`}
    </div>
  );
}

/**
 * The footer row of the activity popover. The caller owns the feed (the pill needs the panel
 * while the popover is closed), so the row takes its load state; it expands in place, and a
 * question that arrives expands it, since the popover has no nested popover to open.
 */
export function SummaryStatusRow({
  load,
  reload,
  announce = true,
}: {
  load: PanelLoad;
  reload: () => void;
  /** False when the caller renders QuestionAnnouncement outside the popover that holds this row. */
  announce?: boolean;
}) {
  const question = useAtomValue(summaryQuestionAtom);
  const questionId = question?.id ?? null;
  const [expanded, setExpanded] = useState(false);
  useEffect(() => {
    if (questionId !== null) setExpanded(true);
  }, [questionId]);
  const { label, count } = statusText(load);
  const running = load.status === "ready" && load.panel.running !== null;
  const Chevron = expanded ? ChevronUp : ChevronDown;
  return (
    <>
      <button
        type="button"
        className="activity-row"
        aria-expanded={expanded}
        aria-label={`Summary rebuilds: ${label}`}
        onClick={() => setExpanded((value) => !value)}
      >
        <ActivityTile kind="summaries" paused={!running} />
        <span className="activity-row__body">
          <span className="activity-row__title">Summaries</span>
          <span className="activity-row__sub">{label}</span>
        </span>
        <span className="activity-row__time">
          {count} <Chevron aria-hidden="true" className="activity-row__chevron inline" />
        </span>
      </button>
      {expanded && (
        <div className="activity-foot__detail [overflow-wrap:anywhere]">
          <StatusContent load={load} reload={reload} />
        </div>
      )}
      {announce && <QuestionAnnouncement question={question} />}
    </>
  );
}
