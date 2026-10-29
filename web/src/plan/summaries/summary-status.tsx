import { useAtomValue } from "jotai/react";
import { RefreshCwIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { useNow } from "@/components/hooks/use-now";
import { Button } from "@/components/ui/button";
import type { SummaryPanel as Panel } from "@/plan/api/schema-summaries";
import { Popover, PopoverContent, PopoverTrigger } from "@/plan/header/popover";
import { canAuthorAtom } from "@/plan/state/atoms";
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

function QuestionAnswer({ question }: { question: NonNullable<Panel["question"]> }) {
  const canAnswer = useAtomValue(canAuthorAtom);
  const { answer } = useSummaryActions();
  const [error, setError] = useState<string | null>(null);
  return (
    <>
      <RebuildQuestion
        question={question}
        canAnswer={canAnswer}
        onAnswer={(rebuild) => {
          setError(null);
          void answer(question.id, rebuild).then(setError);
        }}
      />
      {error && <p role="alert">{error}</p>}
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
      <SummaryPanel panel={load.panel} nowSecs={nowSecs} />
      {load.panel.question && (
        <QuestionAnswer key={load.panel.question.id} question={load.panel.question} />
      )}
    </>
  );
}

export function SummaryStatus({ compact = false }: { compact?: boolean }) {
  const { load, reload } = useSummaryFeed();
  const question = useAtomValue(summaryQuestionAtom);
  const [open, setOpen] = useState(false);
  const lastAnnouncedId = useRef<string | null>(null);

  useEffect(() => {
    if (!question || question.id === lastAnnouncedId.current) return;
    lastAnnouncedId.current = question.id;
    setOpen(true);
  }, [question]);

  return (
    <>
      <Popover open={open} onOpenChange={setOpen}>
        <StatusTrigger compact={compact} load={load} />
        <PopoverContent
          aria-label="Summary rebuilds"
          collisionPadding={16}
          className="max-h-[80vh] w-[calc(100vw-2rem)] min-w-0 overflow-x-hidden overflow-y-auto [overflow-wrap:anywhere] lg:w-96"
          onOpenAutoFocus={(event) => event.preventDefault()}
        >
          <StatusContent load={load} reload={reload} />
        </PopoverContent>
      </Popover>
      <div role="status" aria-live="polite" className="sr-only">
        {question &&
          `Rebuild ${question.count} summaries? Estimated ${question.estimate_tokens.toLocaleString("en-US")} tokens.`}
      </div>
    </>
  );
}
