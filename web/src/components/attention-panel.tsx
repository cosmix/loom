import { cn } from "cn";
import { useAtomValue } from "jotai/react";
import { ChevronRight, History } from "lucide-react";
import { useMemo } from "react";
import { Link, useSearchParams } from "react-router";

import type { Attention, StageSummary } from "@/api/schema";
import { HazardPanel } from "@/aurora-ui/feedback/HazardPanel";
import { BlockedFacts } from "@/components/attention-blocked";
import { attentionDetail, attentionHazard, attentionStatus } from "@/components/attention-status";
import { AttentionSteps, hasSteps } from "@/components/attention-steps";
import { HISTORY_HASH, STAGE_PARAM, stageHref } from "@/components/stage-href";
import { toneClass } from "@/components/state-badge";
import { LinesBox, TextBox } from "@/components/text-box";
import { displayState } from "@/lib/display-state";
import { stateMeta } from "@/lib/format";
import { attentionAtom, stageValueAtom } from "@/state/atoms";

/// The complete review reason while the stage is in review: the stage's
/// `review_notes`. The entry's own `review_reason` is the terminal's one-line,
/// 200-character cut of it.
export function reviewNotesFor(entry: Attention, notes: string | null | undefined): string | null {
  return entry.review_reason === null ? null : (notes ?? null);
}

/// The archived issue count and the complete review notes the card reads off
/// its stage, each a primitive so a snapshot that leaves them unchanged does
/// not re-render the card.
const historyLength = (stage: StageSummary | undefined) => stage?.history.length ?? 0;
const reviewNotes = (stage: StageSummary | undefined) => stage?.review_notes ?? null;
const shownState = (stage: StageSummary | undefined) => (stage ? displayState(stage) : null);

/// One card per attention entry: the ones waiting on a person under "needs
/// attention", the ones loom is resolving under "handled by loom". Each group
/// is hidden when empty, the whole panel when both are.
export function AttentionPanel() {
  const entries = useAtomValue(attentionAtom);
  if (entries.length === 0) return null;
  return (
    <div className="flex flex-col gap-5">
      <AttentionGroup
        titleId="attention-title"
        title="needs attention"
        entries={entries.filter((entry) => !entry.automatic)}
      />
      <AttentionGroup
        titleId="attention-automatic-title"
        title="handled by loom"
        entries={entries.filter((entry) => entry.automatic)}
      />
    </div>
  );
}

function AttentionGroup({
  titleId,
  title,
  entries,
}: {
  titleId: string;
  title: string;
  entries: Attention[];
}) {
  if (entries.length === 0) return null;
  return (
    <section aria-labelledby={titleId} className="@container flex flex-col gap-3">
      <h2 id={titleId} className="eyebrow">
        {title}
      </h2>
      <div className="grid grid-cols-1 gap-3 @3xl:grid-cols-2 @6xl:grid-cols-3">
        {entries.map((entry) => (
          <AttentionCard key={`${entry.id}:${entry.label}`} entry={entry} />
        ))}
      </div>
    </section>
  );
}

/// One stage's card: the label on the caution-tape strip, the stage and what
/// it needs now on the clean body below. Archived issues stay out of it; a
/// link opens them in the stage dialog's history.
function AttentionCard({ entry }: { entry: Attention }) {
  const earlier = useAtomValue(useMemo(() => stageValueAtom(entry.id, historyLength), [entry.id]));
  const notes = useAtomValue(useMemo(() => stageValueAtom(entry.id, reviewNotes), [entry.id]));
  const shown = useAtomValue(useMemo(() => stageValueAtom(entry.id, shownState), [entry.id]));
  const detail = attentionDetail(entry);
  return (
    <HazardPanel
      tone={attentionHazard(entry)}
      title={entry.label}
      headerAction={
        detail && <span className="attention-detail text-xs text-muted-foreground">{detail}</span>
      }
      headerActionClassName="min-w-0 shrink"
      className={cn(
        "min-w-0 bg-card text-foreground",
        toneClass(stateMeta(attentionStatus(entry, shown)).tone),
      )}
      bodyClassName="flex flex-col gap-2.5 text-sm"
    >
      <p>
        <Link to={stageHref(entry.id)} className="font-medium hover:underline">
          {entry.name}
        </Link>{" "}
        <span className="font-mono text-xs wrap-anywhere text-muted-foreground">{entry.id}</span>
      </p>
      <AttentionBody entry={entry} notes={reviewNotesFor(entry, notes)} />
      {earlier > 0 && <EarlierIssues id={entry.id} count={earlier} />}
    </HazardPanel>
  );
}

/// Opens the stage dialog at its history.
function EarlierIssues({ id, count }: { id: string; count: number }) {
  const [params] = useSearchParams();
  const next = new URLSearchParams(params);
  next.set(STAGE_PARAM, id);
  return (
    <Link to={{ search: `?${next.toString()}`, hash: HISTORY_HASH }} className="earlier-issues">
      <History aria-hidden="true" className="size-3.5 text-muted-foreground" />
      {count} earlier {count === 1 ? "issue" : "issues"}
      <ChevronRight aria-hidden="true" className="size-3 text-muted-foreground" />
    </Link>
  );
}

/// What the stage needs now, shown by the attention card and the stage
/// report's now card: the evidence, the block's facts, the reason and note
/// verbatim, the numbered commands and the cleanup warning. `notes` is the
/// complete review reason when the stage has one; it replaces the cut
/// `reason`. `heading` adds the "what to do" (or "what loom is doing") title
/// the stage report puts over the guidance.
export function AttentionBody({
  entry,
  notes = null,
  heading = false,
}: {
  entry: Attention;
  notes?: string | null;
  heading?: boolean;
}) {
  const reason =
    notes === null && entry.reason && !entry.note?.includes(entry.reason) ? entry.reason : null;
  const spoken = notes !== null || reason !== null || Boolean(entry.note);
  return (
    <>
      {entry.evidence.length > 0 && <LinesBox label="evidence" lines={entry.evidence} />}
      <BlockedFacts entry={entry} />
      {heading && (spoken || hasSteps(entry)) && (
        <h3 className="eyebrow">{entry.automatic ? "what loom is doing" : "what to do"}</h3>
      )}
      {spoken && (
        <div className="attention-note">
          <span aria-hidden="true" className="attention-note-dot" />
          <div className="flex min-w-0 flex-col gap-1.5">
            {notes !== null && (
              <TextBox label="review notes text" className="text-box-prose whitespace-pre-wrap">
                {notes}
              </TextBox>
            )}
            {reason !== null && <p className="break-words whitespace-pre-wrap">{reason}</p>}
            {entry.note && <p className="break-words whitespace-pre-wrap">{entry.note}</p>}
          </div>
        </div>
      )}
      <AttentionSteps entry={entry} />
      {entry.cleanup_warning && (
        <p className={cn("text-xs wrap-anywhere", toneClass("warning"))}>{entry.cleanup_warning}</p>
      )}
    </>
  );
}
