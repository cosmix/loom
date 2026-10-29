import { useAtomValue, useSetAtom } from "jotai/react";
import { useEffect } from "react";
import { Link, useParams } from "react-router";

import type { ChecklistView } from "@/plan/api/schema";
import { Untrusted } from "@/plan/chat/visible-controls";
import { ChecklistGaps } from "@/plan/checklist-view";
import { DraftTeamPanel } from "@/plan/feature/draft-team-panel";
import { StepList } from "@/plan/feature/step-list";
import { isCurrentStep, readyLabel } from "@/plan/header/step-tracker";
import { activeFeatureAtom, featuresAtom, projectAtom } from "@/plan/state/atoms";
import { LoadState } from "@/plan/ui/load-state";

function decodeFeature(feature: string): string {
  try {
    return decodeURIComponent(feature);
  } catch {
    return feature;
  }
}

function heading(checklist: ChecklistView): string {
  const ready = readyLabel(checklist.current);
  if (ready) return ready;
  const step = checklist.steps.find((candidate) => isCurrentStep(checklist.current, candidate));
  return step ? `Step ${step.number} of ${checklist.steps.length}: ${step.name}` : "Planning";
}

function UnknownFeature({ name }: { name: string }) {
  return (
    <div className="space-y-3">
      <LoadState state="error" title="No such feature" />
      <p>
        <Untrusted text={name} /> does not exist.
      </p>
      <Link to="/" className="text-sm underline">
        Back to home
      </Link>
    </div>
  );
}

export function FeatureRoute() {
  const { feature } = useParams();
  const project = useAtomValue(projectAtom);
  const features = useAtomValue(featuresAtom);
  const setActiveFeature = useSetAtom(activeFeatureAtom);
  const decodedFeature = decodeFeature(feature ?? "");
  // Router params are normally decoded already; preserve names containing literal escapes.
  const checklist =
    features.find((candidate) => candidate.feature === feature) ??
    features.find((candidate) => candidate.feature === decodedFeature);
  const name = checklist?.feature;

  useEffect(() => {
    if (name !== undefined) setActiveFeature(name);
  }, [name, setActiveFeature]);

  if (project === null) return <LoadState state="loading" title="Loading the feature" />;
  if (!checklist) return <UnknownFeature name={decodedFeature} />;
  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-6 p-6">
      <h1 className="font-display text-2xl">
        <Untrusted text={checklist.feature} />
      </h1>
      <p className="text-sm text-muted-foreground">
        <Untrusted text={heading(checklist)} />
      </p>
      <StepList checklist={checklist} />
      <DraftTeamPanel feature={checklist.feature} />
      {checklist.items.length > 0 && <ChecklistGaps checklist={checklist} />}
    </div>
  );
}
