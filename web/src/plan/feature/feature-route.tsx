import { useAtomValue, useSetAtom } from "jotai/react";
import { useEffect } from "react";
import { Link, useParams, useSearchParams } from "react-router";

import type { ChecklistView } from "@/plan/api/schema";
import { Untrusted } from "@/plan/chat/visible-controls";
import { DraftTeamPanel } from "@/plan/feature/draft-team-panel";
import { PaceChoice } from "@/plan/feature/pace";
import { Queue, remainingItems } from "@/plan/feature/queue";
import { StepItems } from "@/plan/feature/step-items";
import { StepList } from "@/plan/feature/step-list";
import { selectedStep, STEP_COUNT, stepTitle } from "@/plan/feature/steps";
import { activeFeatureAtom, featuresAtom, projectAtom } from "@/plan/state/atoms";
import { LoadState } from "@/plan/ui/load-state";

import "./feature.css";

function decodeFeature(feature: string): string {
  try {
    return decodeURIComponent(feature);
  } catch {
    return feature;
  }
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

function StepBody({ checklist, selected }: { checklist: ChecklistView; selected: number }) {
  const step = checklist.steps.find((candidate) => candidate.number === selected);
  return (
    <div className="plan-feature__body">
      <header className="plan-feature__head">
        <p className="plan-eyebrow">
          <Untrusted text={checklist.feature} /> · step {selected} of {STEP_COUNT}
        </p>
        <h1 className="plan-feature__title">{stepTitle(selected)}</h1>
      </header>
      {step && <PaceChoice checklist={checklist} step={step} />}
      <DraftTeamPanel feature={checklist.feature} />
      {selected === 2 && <Queue checklist={checklist} kind="section" />}
      {selected === 4 && <Queue checklist={checklist} kind="plan" />}
      <StepItems items={remainingItems(checklist, selected)} />
    </div>
  );
}

export function FeatureRoute() {
  const { feature } = useParams();
  const [search] = useSearchParams();
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
  const selected = selectedStep(search.get("step"), checklist);
  return (
    <div className="plan-feature">
      <StepList checklist={checklist} selected={selected} />
      <StepBody checklist={checklist} selected={selected} />
    </div>
  );
}
