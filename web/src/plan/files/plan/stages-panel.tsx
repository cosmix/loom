import { useState } from "react";

import type { PlanView, StageRow as StageRowData } from "@/plan/api/schema-files";
import { StageGraph } from "@/plan/files/plan/stage-graph";
import { StageRow } from "@/plan/files/plan/stage-row";

function scrollToStage(id: string): void {
  document.getElementById(`stage:${id}`)?.scrollIntoView({ block: "nearest" });
}

/// DOM id suffix per stage: empty for the first stage with an id (other code addresses stages
/// by `stage:<id>`), the row index for each later repeat of it.
function idSuffixes(stages: StageRowData[]): string[] {
  const seen = new Set<string>();
  return stages.map((stage, index) => {
    if (!seen.has(stage.id)) {
      seen.add(stage.id);
      return "";
    }
    return `:${index}`;
  });
}

/// Stands in for the plan's metadata block: the stage graph and one row per
/// stage, or the YAML itself on request.
export function StagesPanel({ plan }: { plan: PlanView }) {
  const [showYaml, setShowYaml] = useState(false);

  if (plan.yaml_error !== null) {
    return (
      <section aria-label="Stages" className="my-4 space-y-2">
        <p role="alert" className="text-[13px] text-destructive">
          {plan.yaml_error}
        </p>
        {plan.yaml !== null && <pre className="overflow-x-auto font-mono text-xs">{plan.yaml}</pre>}
      </section>
    );
  }
  if (plan.yaml === null) {
    return (
      <section aria-label="Stages" className="my-4 text-[13px] text-muted-foreground">
        No metadata block yet.
      </section>
    );
  }
  const suffixes = idSuffixes(plan.stages);
  return (
    <section aria-label="Stages" className="my-4 space-y-3">
      <div className="flex items-baseline justify-between">
        <h2 className="text-[13px] font-medium">Stages</h2>
        <button
          type="button"
          onClick={() => setShowYaml((value) => !value)}
          className="text-[13px] underline-offset-2 hover:underline"
        >
          {showYaml ? "Show stages" : "Show YAML"}
        </button>
      </div>
      {showYaml ? (
        <pre className="overflow-x-auto rounded-md border border-(--hairline) p-3 font-mono text-xs">
          <code>{plan.yaml}</code>
        </pre>
      ) : (
        <>
          {plan.stages.length > 0 && <StageGraph stages={plan.stages} onSelect={scrollToStage} />}
          <ul>
            {plan.stages.map((stage, index) => (
              <StageRow key={`${index}:${stage.id}`} stage={stage} idSuffix={suffixes[index]} />
            ))}
          </ul>
        </>
      )}
    </section>
  );
}
