import { useState } from "react";

import { ErrorPanel } from "@/aurora-ui/feedback/HazardPanel";
import type { PlanView, StageRow as StageRowData } from "@/plan/api/schema-files";
import { Untrusted } from "@/plan/chat/visible-controls";
import { StageGraph } from "@/plan/files/plan/stage-graph";
import { StageRow, type StageLinks } from "@/plan/files/plan/stage-row";
import { HEADING_BUTTON } from "@/plan/ui/doc-heading";

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

function MetadataNotice({ plan }: { plan: PlanView }) {
  if (plan.yaml_error !== null) {
    return (
      <section aria-label="Stages" className="my-4 space-y-2">
        <div role="alert">
          <ErrorPanel
            title="Plan metadata could not be read"
            message={<Untrusted text={plan.yaml_error} />}
          />
        </div>
        {plan.yaml !== null && <pre className="overflow-x-auto font-mono text-xs">{plan.yaml}</pre>}
      </section>
    );
  }
  return (
    <section aria-label="Stages" className="my-4 text-[13px] text-muted-foreground">
      No metadata block yet.
    </section>
  );
}

/// Stands in for the plan's metadata block: the stage graph and one row per stage, or the YAML.
export function StagesPanel({ plan, links }: { plan: PlanView; links: StageLinks }) {
  const [showYaml, setShowYaml] = useState(false);
  if (plan.yaml_error !== null || plan.yaml === null) return <MetadataNotice plan={plan} />;
  const suffixes = idSuffixes(plan.stages);
  return (
    <section aria-label="Stages" className="my-6 space-y-3">
      <div className="flex items-baseline justify-between">
        <h2 className="text-(length:--type-title) font-semibold">Stages</h2>
        <button type="button" {...HEADING_BUTTON} onClick={() => setShowYaml((value) => !value)}>
          {showYaml ? "Show stages" : "Show YAML"}
        </button>
      </div>
      {showYaml ? (
        <pre className="overflow-x-auto rounded-md bg-muted p-3 font-mono text-xs">
          <code>{plan.yaml}</code>
        </pre>
      ) : (
        <>
          {plan.stages.length > 0 && <StageGraph stages={plan.stages} onSelect={scrollToStage} />}
          <ul>
            {plan.stages.map((stage, index) => (
              <StageRow
                key={`${index}:${stage.id}`}
                stage={stage}
                links={links}
                idSuffix={suffixes[index]}
              />
            ))}
          </ul>
        </>
      )}
    </section>
  );
}
