import { cn } from "cn";

import type { Attention, StageSummary } from "@/api/schema";
import { AttentionBody, reviewNotesFor } from "@/components/attention-panel";
import { attentionStatus } from "@/components/attention-status";
import { Moment } from "@/components/stage-history";
import { currentFailure } from "@/components/stage-history-model";
import { StateGlyph, toneClass } from "@/components/state-badge";
import { displayState } from "@/lib/display-state";
import { stateMeta } from "@/lib/format";
import { DISPLAY_STATES } from "@/lib/states";

/// The state's legend from the shared state table, as a sentence about this
/// stage: a legend's `<id>` placeholder becomes the stage's id.
function stateSentence(stage: StageSummary): string {
  const legend = DISPLAY_STATES[displayState(stage)].legend.replaceAll("<id>", stage.id);
  return `${legend.charAt(0).toUpperCase()}${legend.slice(1)}.`;
}

/// The entry as the now card shows it. An entry for something other than the
/// failure (unrestored stash, failed cleanup, a completion blocker) carries no
/// evidence of its own, so the stage's live failure lends it its evidence.
function withFailure(stage: StageSummary, entry: Attention): Attention {
  const failure = currentFailure(stage);
  if (entry.evidence.length > 0 || failure === null) return entry;
  return { ...entry, evidence: failure.evidence };
}

/// Where the stage stands now: the attention label naming the issue, the
/// state in one sentence, then what its attention entry carries (evidence,
/// the block's facts, the reason and note verbatim, the numbered commands,
/// the cleanup warning). The only place a stage's "what to do" appears. It
/// wears the entry's tone, as the attention card does. The report renders it
/// only for a stage with an entry. "since" dates the live failure, and only
/// while its evidence is on the card.
export function NowCard({ stage, entry }: { stage: StageSummary; entry: Attention }) {
  const shown = withFailure(stage, entry);
  const failure = currentFailure(stage);
  const since = shown.evidence.length > 0 ? (failure?.detected_at ?? null) : null;
  const status = attentionStatus(entry, displayState(stage));
  return (
    <section
      aria-label="now"
      className={cn("ledger-card now-card text-foreground", toneClass(stateMeta(status).tone))}
    >
      <header className="ledger-head">
        <span className="ledger-icon">
          <StateGlyph status={status} className="text-xs" />
        </span>
        <h2 className="eyebrow">now</h2>
        <span className="now-label">{entry.label}</span>
        {since !== null && (
          <span className="now-since">
            since <Moment at={since} />
          </span>
        )}
      </header>
      <div className="now-body">
        <p className="leading-normal">{stateSentence(stage)}</p>
        <AttentionBody entry={shown} notes={reviewNotesFor(shown, stage.review_notes)} heading />
      </div>
    </section>
  );
}
