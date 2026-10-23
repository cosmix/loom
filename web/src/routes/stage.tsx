import { useAtomValue } from "jotai/react";
import { ArrowLeftIcon, SearchXIcon } from "lucide-react";
import { Link, useNavigate, useParams } from "react-router";

import { EmptyState } from "@/aurora-ui/feedback/EmptyState";

import { OperatorGuidance } from "@/components/attention-panel";
import { CopyCommand } from "@/components/copy-command";
import { StageHeading, ThreadRows } from "@/components/stage-heading";
import { stageHref } from "@/components/stage-href";
import { StageSectionGrid } from "@/components/stage-sections";
import { TerminalMark, terminalGate, terminalHistory } from "@/components/terminal/terminal-glyph";
import { terminalHref } from "@/components/terminal/terminal-view";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { attentionAtom, orderedStagesAtom, selectStage, snapshotAtom } from "@/state/atoms";

/// Route `/stages/:stageId`: everything the row shows plus every remaining
/// `StageSummary` field, grouped.
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
  const gate = terminalGate(snapshot.terminals, stage);
  const history = terminalHistory(snapshot.terminals, stage) !== null;

  return (
    <article className="mx-auto flex w-full max-w-[1440px] flex-col gap-5 px-4 py-5 sm:px-6">
      <BackLink />
      <div className="flex items-start gap-4">
        <div className="min-w-0 flex-1">
          <StageHeading stage={stage} />
        </div>
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
          {snapshot.terminals && stage.session_backend === "native" && (
            <CopyCommand command="loom run --backend tmux" />
          )}
        </div>
      </div>
      <ThreadRows stage={stage} pick={(id) => void navigate(stageHref(id))} />
      {attention && <OperatorGuidance entry={attention} />}
      <StageSectionGrid stage={stage} level={level} wide />
    </article>
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
