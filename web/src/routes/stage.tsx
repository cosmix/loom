import { useAtomValue } from "jotai/react";
import { ArrowLeftIcon, SearchXIcon } from "lucide-react";
import { Link, useNavigate, useParams } from "react-router";

import type { StageSummary } from "@/api/schema";
import { EmptyState } from "@/aurora-ui/feedback/EmptyState";

import { CopyCommand } from "@/components/copy-command";
import { StageHeading, ThreadChips } from "@/components/stage-heading";
import { stageHref } from "@/components/stage-href";
import { StageReport } from "@/components/stage-report";
import { TerminalMark, terminalGate, terminalHistory } from "@/components/terminal/terminal-glyph";
import { terminalHref } from "@/components/terminal/terminal-view";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { attentionAtom, orderedStagesAtom, selectStage, snapshotAtom } from "@/state/atoms";

/// Route `/stages/:stageId`: the dialog's report at page width, details open,
/// since a page has the room for every remaining `StageSummary` field.
export function StagePage() {
  const { stageId = "" } = useParams<{ stageId: string }>();
  const snapshot = useAtomValue(snapshotAtom);
  const ordered = useAtomValue(orderedStagesAtom);
  const attention = useAtomValue(attentionAtom).find((entry) => entry.id === stageId);
  const navigate = useNavigate();

  if (snapshot === null) return <StageSkeleton />;
  const stage = selectStage(snapshot, stageId);
  if (stage === undefined) return <NotFound id={stageId} />;
  const level = ordered.find((entry) => entry.stage.id === stageId)?.level ?? null;

  return (
    <article className="mx-auto flex w-full max-w-[1440px] flex-col gap-5 px-4 py-5 sm:px-6">
      <BackLink />
      <div className="flex items-start gap-4">
        <div className="min-w-0 flex-1">
          <StageHeading stage={stage} />
        </div>
        <StageActions stage={stage} terminals={snapshot.terminals} />
      </div>
      <ThreadChips stage={stage} pick={(id) => void navigate(stageHref(id))} />
      <StageReport stage={stage} entry={attention} level={level} wide detailsOpen />
    </article>
  );
}

/// The way into the stage's terminal, or its saved history once the session
/// ended; disabled, with the reason on hover, when neither is open to it.
function StageActions({ stage, terminals }: { stage: StageSummary; terminals: boolean }) {
  const gate = terminalGate(terminals, stage);
  const history = terminalHistory(terminals, stage) !== null;
  return (
    <div className="flex shrink-0 items-center gap-2">
      {gate === null || history ? (
        <Button variant="outline" size="sm" asChild>
          <Link
            to={terminalHref(stage.id)}
            aria-label={history ? "open terminal history" : "open terminal"}
          >
            <TerminalMark className="size-3.5" />
            {history ? "Terminal history" : "Terminal"}
          </Link>
        </Button>
      ) : (
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled
          aria-label="open terminal"
          title={gate}
        >
          <TerminalMark className="size-3.5" />
          Terminal
        </Button>
      )}
      {terminals && stage.session_backend === "native" && (
        <CopyCommand command="loom run --backend tmux" />
      )}
    </div>
  );
}

function BackLink() {
  return (
    <Link
      to="/"
      className="inline-flex w-fit items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground"
    >
      <ArrowLeftIcon className="size-4" />
      overview
    </Link>
  );
}

export function NotFound({ id }: { id: string }) {
  return (
    <div className="mx-auto flex w-full max-w-[1440px] flex-col gap-3 px-4 py-5 sm:px-6">
      <BackLink />
      <EmptyState
        icon={SearchXIcon}
        title="No such stage"
        description={
          <>
            No stage named <code className="rounded bg-muted px-1 py-0.5 text-xs">{id}</code> in
            this workspace.
          </>
        }
        tone="muted"
      />
    </div>
  );
}

function StageSkeleton() {
  return (
    <div
      className="mx-auto flex w-full max-w-[1440px] flex-col gap-4 px-4 py-5 sm:px-6"
      aria-busy="true"
    >
      <Skeleton className="h-4 w-16" />
      <Skeleton className="h-7 w-64" />
      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
        <Skeleton className="h-36" />
        <Skeleton className="h-36" />
        <Skeleton className="h-36" />
      </div>
    </div>
  );
}
