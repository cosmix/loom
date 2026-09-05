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
/// (`loom/src/commands/status/render/attention_model.rs:79-85`). It omits
/// "BLOCKED" and "CLEANUP FAILED" deliberately — they fall through to the
/// `"blocked"` default below and land on the right tone and glyph anyway. A
/// label rename on the Rust side desyncs this map with no compile-time
/// signal, so check it there before adding new labels.
const LABEL_STATUS: Record<string, StageStatus> = {
  "MERGE CONFLICT": "merge-conflict",
  "ACCEPTANCE FAILED": "completed-with-failures",
  "MERGE ERROR": "merge-blocked",
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

export function attentionHazard(entry: Attention): "error" | "warning" {
  return hazardTone(attentionStatus(entry)) ?? "error";
}

/// One card per stage that needs a person; hidden when nothing does.
export function AttentionPanel() {
  const entries = useAtomValue(attentionAtom);
  if (entries.length === 0) return null;
  return (
    <section aria-labelledby="attention-title" className="@container flex flex-col gap-3">
      <h2 id="attention-title" className="eyebrow">
        needs attention
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

/// The evidence, the command, the review choices, and the cleanup warning:
/// what the card and the stage dialog both show under their strips.
export function AttentionBody({ entry }: { entry: Attention }) {
  return (
    <>
      {entry.evidence.length > 0 && <Evidence lines={entry.evidence} />}
      <div className="flex flex-col gap-1.5 text-foreground">
        <CopyCommand command={entry.hint} />
        {entry.has_human_review_choices && <ReviewChoices id={entry.id} />}
      </div>
      {entry.cleanup_warning && (entry.review_reason || entry.failure_type) && (
        <p className={cn("text-xs", toneClass("warning"))}>{entry.cleanup_warning}</p>
      )}
    </>
  );
}

/// `review_reason`, else the failure label, else the cleanup warning — the
/// TUI's `attention_detail`, with the adjudication numbers appended.
export function attentionDetail(entry: Attention): string | null {
  const parts: string[] = [];
  if (entry.review_reason) parts.push(entry.review_reason);
  else if (entry.failure_type) parts.push(entry.failure_label ?? failureLabel(entry.failure_type));
  else if (entry.cleanup_warning) parts.push(entry.cleanup_warning);
  if (entry.dispute_count !== null) parts.push(`${entry.dispute_count} disputed`);
  if (entry.judge_heartbeat_secs !== null) {
    parts.push(`judge heard ${formatElapsed(entry.judge_heartbeat_secs)} ago`);
  }
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
