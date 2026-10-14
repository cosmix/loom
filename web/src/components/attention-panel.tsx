import { cn } from "cn";
import { useAtomValue } from "jotai/react";
import { Link } from "react-router";

import type { Attention, StageStatus } from "@/api/schema";
import { HazardPanel } from "@/aurora-ui/feedback/HazardPanel";
import { CopyCommand } from "@/components/copy-command";
import { stageHref } from "@/components/stage-href";
import { toneClass } from "@/components/state-badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { failureLabel, formatElapsed, hazardTone } from "@/lib/format";
import { attentionAtom } from "@/state/atoms";

/// The state each attention label stands for (`panels.rs` `entry_status`).
/// Computed here because no formatter exposes it.
///
/// Keyed on the Rust side's literal attention-label strings
/// (`status_guidance` in `loom/src/commands/status/render/attention_model.rs`). It omits
/// "BLOCKED", "CLEANUP FAILED" and "STASH NOT RESTORED" deliberately — they fall through to the
/// `"blocked"` default below and land on the right tone and glyph anyway. A
/// label rename on the Rust side desyncs this map with no compile-time
/// signal, so check it there before adding new labels.
const LABEL_STATUS: Record<string, StageStatus> = {
  "MERGE CONFLICT": "merge-conflict",
  "ACCEPTANCE FAILED": "completed-with-failures",
  "MERGE ERROR": "merge-blocked",
  "MERGE BLOCKED": "merge-blocked",
  "NEEDS REVIEW": "needs-human-review",
  "NEEDS INPUT": "waiting-for-input",
  ADJUDICATING: "needs-adjudication",
};

/// The three decisions `loom stage human-review` accepts (`human_review.rs`).
const REVIEW_CHOICES = ["--approve", "--force-complete", '--reject "<reason>"'] as const;

/// The state behind an attention entry, and the caution tape it wears.
export function attentionStatus(entry: Attention): StageStatus {
  return LABEL_STATUS[entry.label] ?? "blocked";
}

/// A state loom is resolving itself is never an error for the operator.
export function attentionHazard(entry: Attention): "error" | "warning" {
  if (entry.automatic) return "warning";
  return hazardTone(attentionStatus(entry)) ?? "error";
}

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

/// One stage's card: the label on the caution-tape strip, the stage and its
/// evidence and commands on the clean body below.
function AttentionCard({ entry }: { entry: Attention }) {
  const detail = attentionDetail(entry);
  return (
    <HazardPanel
      tone={attentionHazard(entry)}
      title={entry.label}
      headerAction={
        detail && (
          <span className="max-w-40 truncate text-xs text-muted-foreground" title={detail}>
            {detail}
          </span>
        )
      }
      className="min-w-0 bg-card"
      bodyClassName="flex flex-col gap-2.5 text-sm"
    >
      <p>
        <Link to={stageHref(entry.id)} className="font-medium hover:underline">
          {entry.name}
        </Link>{" "}
        <span className="font-mono text-xs text-muted-foreground">{entry.id}</span>
      </p>
      <AttentionBody entry={entry} />
    </HazardPanel>
  );
}

/// What the stage dialog and the stage page tell the operator to do, or what
/// loom is doing about the stage.
export function OperatorGuidance({ entry }: { entry: Attention }) {
  return (
    <div className="flex flex-col gap-2.5 rounded-lg border border-hairline bg-card p-4 text-sm">
      <h2 className="eyebrow">{entry.automatic ? "what loom is doing" : "what to do"}</h2>
      <AttentionBody entry={entry} />
    </div>
  );
}

/// The evidence, the note, the command, the other options, the review
/// choices, and the cleanup warning: what the card and the stage dialog both
/// show under their strips. The note is prose, so it never gets a copy button,
/// and it is shown whole: it may be a long reason with its own line breaks.
export function AttentionBody({ entry }: { entry: Attention }) {
  return (
    <>
      {entry.evidence.length > 0 && <Evidence lines={entry.evidence} />}
      {entry.note && <p className="break-words whitespace-pre-wrap">{entry.note}</p>}
      {(entry.command || entry.has_human_review_choices) && (
        <div className="flex flex-col gap-1.5 text-foreground">
          {entry.command && (
            <Step
              lead={entry.command_when ? `Once ${entry.command_when}:` : null}
              command={entry.command}
            />
          )}
          {entry.has_human_review_choices && <ReviewChoices id={entry.id} />}
        </div>
      )}
      {entry.alternatives.length > 0 && (
        <ul className="flex flex-col gap-2.5 text-foreground" aria-label="other options">
          {entry.alternatives.map((alternative) => (
            <li key={alternative.command}>
              <Step lead={`If ${alternative.when}:`} command={alternative.command} />
            </li>
          ))}
        </ul>
      )}
      {entry.cleanup_warning && (entry.review_reason || entry.failure_type) && (
        <p className={cn("text-xs", toneClass("warning"))}>{entry.cleanup_warning}</p>
      )}
    </>
  );
}

/// A command with the sentence that says when to run it.
function Step({ lead, command }: { lead: string | null; command: string }) {
  return (
    <div className="flex flex-col gap-1">
      {lead && <p className="text-xs break-words text-muted-foreground">{lead}</p>}
      <CopyCommand command={command} />
    </div>
  );
}

/// The adjudication numbers alone: dispute count and judge heartbeat age.
export function attentionCounts(entry: Attention): string | null {
  const parts: string[] = [];
  if (entry.dispute_count !== null) parts.push(`${entry.dispute_count} disputed`);
  if (entry.judge_heartbeat_secs !== null) {
    parts.push(`judge heard ${formatElapsed(entry.judge_heartbeat_secs)} ago`);
  }
  return parts.length > 0 ? parts.join(" · ") : null;
}

/// `review_reason`, else the failure label, else the cleanup warning — the
/// TUI's `attention_detail`, with the adjudication numbers appended.
export function attentionDetail(entry: Attention): string | null {
  const parts: string[] = [];
  if (entry.review_reason) parts.push(entry.review_reason);
  else if (entry.failure_type) parts.push(entry.failure_label ?? failureLabel(entry.failure_type));
  else if (entry.cleanup_warning) parts.push(entry.cleanup_warning);
  const counts = attentionCounts(entry);
  if (counts !== null) parts.push(counts);
  return parts.length > 0 ? parts.join(" · ") : null;
}

/// Untrusted text: rendered as text nodes only, never as markup.
function Evidence({ lines }: { lines: string[] }) {
  return (
    <ScrollArea className="rounded-md border border-hairline bg-background [&>[data-slot=scroll-area-viewport]]:max-h-32">
      <pre className="p-2.5 text-[11px] leading-relaxed whitespace-pre-wrap text-foreground">
        {lines.map((line, index) => (
          <span key={index} className="block">
            {line}
          </span>
        ))}
      </pre>
    </ScrollArea>
  );
}

function ReviewChoices({ id }: { id: string }) {
  return (
    <ul className="flex flex-wrap gap-1.5" aria-label="review decisions">
      {REVIEW_CHOICES.map((choice) => (
        <li key={choice} className="max-w-full min-w-0">
          <CopyCommand command={`loom stage human-review ${id} ${choice}`} />
        </li>
      ))}
    </ul>
  );
}
