import { ChevronRight, Fingerprint } from "lucide-react";
import { useId, useState, type ReactNode } from "react";

import type { StageSummary } from "@/api/schema";
import { ContextMeter } from "@/components/context-meter";
import { tokens, yesNo } from "@/components/stage-detail";
import { MERGE_STATES, StageSectionGrid } from "@/components/stage-sections";
import { formatElapsed } from "@/lib/format";

interface Fact {
  label: string;
  value: ReactNode;
}

/// The handful of fields the folded disclosure lists on its face.
function facts(stage: StageSummary, level: number | null): Fact[] {
  const all: (Fact | null)[] = [
    { label: "type", value: stage.stage_type },
    level === null ? null : { label: "level", value: String(level) },
    { label: "model", value: stage.model },
    stage.context_tokens === null
      ? null
      : {
          label: "orchestrator",
          value: (
            <span className="inline-flex flex-wrap items-center gap-2">
              <ContextMeter stage={stage} />
              <span>{tokens(stage.context_tokens)} tokens</span>
            </span>
          ),
        },
    stage.elapsed_secs === null
      ? null
      : { label: "elapsed", value: formatElapsed(stage.elapsed_secs) },
    stage.base_branch === null ? null : { label: "base", value: stage.base_branch },
    MERGE_STATES.has(stage.status) || stage.merged
      ? { label: "merged", value: stage.merge_assumed ? "assumed" : yesNo(stage.merged) }
      : null,
    stage.acceptance === undefined || stage.acceptance.length === 0
      ? null
      : { label: "acceptance", value: `${stage.acceptance.length} criteria` },
  ];
  return all.filter((fact): fact is Fact => fact !== null);
}

/// Everything else about the stage (identity, timing, session, acceptance,
/// merge, tokens) behind one disclosure, folded unless `defaultOpen`.
export function StageDetails({
  stage,
  level,
  wide = false,
  defaultOpen = false,
}: {
  stage: StageSummary;
  level: number | null;
  wide?: boolean;
  defaultOpen?: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen);
  const panel = useId();
  const summary = useId();
  return (
    <section aria-label="details" className="ledger-card">
      <header className="ledger-head details-head">
        <button
          type="button"
          className="details-toggle"
          aria-label="details"
          aria-describedby={summary}
          aria-expanded={open}
          aria-controls={open ? panel : undefined}
          onClick={() => setOpen((value) => !value)}
        >
          <ChevronRight aria-hidden="true" className="disclosure-chevron" />
          <span className="ledger-icon">
            <Fingerprint aria-hidden="true" className="size-3" />
          </span>
          <span className="eyebrow">details</span>
          <span id={summary} className="details-facts">
            {facts(stage, level).map((fact) => (
              <span key={fact.label} className="details-fact">
                <span className="eyebrow">{fact.label}</span>
                <span className="font-mono text-xs wrap-anywhere">{fact.value}</span>
              </span>
            ))}
          </span>
        </button>
      </header>
      {open && (
        <div id={panel} className="details-panel">
          <StageSectionGrid stage={stage} level={level} wide={wide} />
        </div>
      )}
    </section>
  );
}
