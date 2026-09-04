import { cn } from "cn";
import { useAtomValue } from "jotai/react";
import { Link } from "react-router";

import type { Attention, StageStatus } from "@/api/schema";
import { CopyCommand } from "@/components/copy-command";
import { stageHref } from "@/components/stage-href";
import { StateGlyph, toneClass } from "@/components/state-badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { failureLabel, formatElapsed, stateMeta } from "@/lib/format";
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

/// One card per stage that needs a person; hidden when nothing does.
export function AttentionPanel() {
  const entries = useAtomValue(attentionAtom);
  if (entries.length === 0) return null;
  return (
    <section aria-labelledby="attention-title" className="flex flex-col gap-3">
      <h2 id="attention-title" className="eyebrow">
        needs attention
      </h2>
      <div className="grid gap-3 md:grid-cols-2 2xl:grid-cols-3">
        {entries.map((entry) => (
          <AttentionCard key={`${entry.id}:${entry.label}`} entry={entry} />
        ))}
      </div>
    </section>
  );
}

function AttentionCard({ entry }: { entry: Attention }) {
  const status = LABEL_STATUS[entry.label] ?? "blocked";
  const tone = stateMeta(status).tone;
  const detail = attentionDetail(entry);
  return (
    <article
      className={cn(
        "flex flex-col gap-2.5 rounded-lg border border-hairline border-l-2 border-l-(--tone) bg-card p-3.5 text-sm",
        toneClass(tone),
      )}
    >
      <header className="flex flex-wrap items-baseline gap-x-2 gap-y-1">
        <StateGlyph status={status} />
        <span className="font-semibold tracking-wide">{entry.label}</span>
        {detail && <span className="text-muted-foreground">· {detail}</span>}
      </header>
      <p className="text-foreground">
        <Link to={stageHref(entry.id)} className="font-medium hover:underline">
          {entry.name}
        </Link>{" "}
        <span className="font-mono text-xs text-muted-foreground">{entry.id}</span>
      </p>
      {entry.evidence.length > 0 && <Evidence lines={entry.evidence} />}
      <div className="flex flex-col gap-1.5 text-foreground">
        <CopyCommand command={entry.hint} />
        {entry.has_human_review_choices && <ReviewChoices id={entry.id} />}
      </div>
      {entry.cleanup_warning && (entry.review_reason || entry.failure_type) && (
        <p className={cn("text-xs", toneClass("warning"))}>{entry.cleanup_warning}</p>
      )}
    </article>
  );
}

/// `review_reason`, else the failure label, else the cleanup warning — the
/// TUI's `attention_detail`, with the adjudication numbers appended.
function attentionDetail(entry: Attention): string | null {
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
        <li key={choice}>
          <CopyCommand command={`loom stage human-review ${id} ${choice}`} />
        </li>
      ))}
    </ul>
  );
}
