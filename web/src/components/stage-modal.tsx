import { cn } from "cn";
import { useAtomValue } from "jotai/react";
import { ExternalLinkIcon } from "lucide-react";
import { useCallback } from "react";
import { Link, useSearchParams } from "react-router";

import type { StageSummary } from "@/api/schema";
import { HazardHeader } from "@/aurora-ui/feedback/HazardPanel";
import { AttentionBody, attentionDetail, attentionHazard } from "@/components/attention-panel";
import { StateLine, ThreadRows } from "@/components/stage-heading";
import { stageHref } from "@/components/stage-href";
import { StageSectionGrid } from "@/components/stage-sections";
import { toneClass } from "@/components/state-badge";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Kbd } from "@/components/ui/kbd";
import { stateMeta } from "@/lib/format";
import { attentionAtom, orderedStagesAtom, selectStage, snapshotAtom } from "@/state/atoms";

/// The query parameter naming the stage the modal shows, so an open modal
/// has a URL and the back button closes it.
export const STAGE_PARAM = "stage";

export function useOpenStage(): (id: string) => void {
  const [, setParams] = useSearchParams();
  return useCallback(
    (id: string) =>
      setParams((params) => {
        const next = new URLSearchParams(params);
        next.set(STAGE_PARAM, id);
        return next;
      }),
    [setParams],
  );
}

/// The stage page's content in a dialog, opened from any view with
/// `?stage=<id>`; upstream and downstream chips move the dialog along the
/// thread without closing it.
export function StageModal() {
  const [params, setParams] = useSearchParams();
  const id = params.get(STAGE_PARAM);
  const snapshot = useAtomValue(snapshotAtom);
  const close = () =>
    setParams((current) => {
      const next = new URLSearchParams(current);
      next.delete(STAGE_PARAM);
      return next;
    });
  const stage = id === null ? undefined : selectStage(snapshot, id);

  return (
    <Dialog
      open={id !== null}
      onOpenChange={(open) => {
        if (!open) close();
      }}
    >
      <DialogContent className="stage-modal gap-0 overflow-hidden p-0 sm:max-w-3xl">
        {id !== null && (stage ? <Body stage={stage} /> : <Missing id={id} />)}
      </DialogContent>
    </Dialog>
  );
}

function Body({ stage }: { stage: StageSummary }) {
  const open = useOpenStage();
  const ordered = useAtomValue(orderedStagesAtom);
  const attention = useAtomValue(attentionAtom).find((entry) => entry.id === stage.id);
  const level = ordered.find((entry) => entry.stage.id === stage.id)?.level ?? null;
  const tone = stateMeta(stage.status).tone;

  const detail = attention && attentionDetail(attention);
  return (
    <>
      {attention && (
        <HazardHeader
          tone={attentionHazard(attention)}
          title={attention.label}
          className="pr-12 sm:pr-12"
          action={
            detail && (
              <span className="max-w-xs truncate text-xs text-muted-foreground" title={detail}>
                {detail}
              </span>
            )
          }
        />
      )}
      <DialogHeader className={cn("stage-modal-head gap-2 p-5 pr-12", toneClass(tone))}>
        <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
          <DialogTitle className="text-xl font-semibold tracking-tight text-foreground">
            {stage.name}
          </DialogTitle>
          <span className="font-mono text-sm text-muted-foreground">{stage.id}</span>
        </div>
        <DialogDescription asChild>
          <StateLine stage={stage} className="text-foreground" />
        </DialogDescription>
        <ThreadRows stage={stage} pick={open} className="mt-1" />
      </DialogHeader>
      <div className="max-h-[62dvh] overflow-y-auto p-5">
        <div className="flex flex-col gap-4">
          {attention && (
            <div className="flex flex-col gap-2.5 rounded-lg border border-hairline bg-card p-4 text-sm">
              <h2 className="eyebrow">what to do</h2>
              <AttentionBody entry={attention} />
            </div>
          )}
          <StageSectionGrid stage={stage} level={level} />
        </div>
      </div>
      <footer className="flex items-center justify-between gap-3 border-t border-hairline bg-muted/40 px-5 py-2.5 text-xs text-muted-foreground">
        <span>
          <Kbd>Esc</Kbd> closes
        </span>
        <Link
          to={stageHref(stage.id)}
          className="inline-flex items-center gap-1.5 text-foreground hover:underline"
        >
          open as a page
          <ExternalLinkIcon className="size-3.5" />
        </Link>
      </footer>
    </>
  );
}

function Missing({ id }: { id: string }) {
  return (
    <DialogHeader className="p-5">
      <DialogTitle>No such stage</DialogTitle>
      <DialogDescription>
        No stage named <code className="rounded bg-muted px-1 py-0.5 text-xs">{id}</code> in this
        workspace.
      </DialogDescription>
    </DialogHeader>
  );
}
