import { useSetAtom } from "jotai/react";
import { useEffect } from "react";
import { Link, Navigate, Outlet } from "react-router";

import { landingPath } from "@/plan/feature/landing";
import { Spine } from "@/plan/feature/spine";
import { useFeature } from "@/plan/feature/use-feature";
import { activeFeatureAtom } from "@/plan/state/atoms";
import { LoadState } from "@/plan/ui/load-state";

import "./feature.css";

function UnknownFeature({ name }: { name: string }) {
  return (
    <LoadState state="not_found" title="No such feature" message={name}>
      <Link to="/" className="underline">
        Back to home
      </Link>
    </LoadState>
  );
}

/** The index route of `/f/:feature`: the page the feature's state calls for. */
export function FeatureLanding() {
  const { checklist } = useFeature();
  if (!checklist) return <LoadState state="loading" title="Loading the feature" />;
  return <Navigate to={landingPath(checklist)} replace />;
}

/**
 * Every `/f/:feature` page: the spine beside the routed page. Until the project arrives the page
 * renders without the spine, so a concept page can start loading, and an empty slot keeps the
 * spine's column, so the page does not move when the spine joins it once the feature is known.
 */
export function FeatureLayout() {
  const { name, checklist, loaded } = useFeature();
  const setActiveFeature = useSetAtom(activeFeatureAtom);
  const slug = checklist?.feature;

  useEffect(() => {
    if (slug !== undefined) setActiveFeature(slug);
  }, [slug, setActiveFeature]);

  if (loaded && !checklist) return <UnknownFeature name={name} />;
  return (
    <div className="plan-layout" data-spine="">
      {checklist ? <Spine /> : <div className="plan-layout__spine-slot" aria-hidden="true" />}
      <div className="plan-layout__page">
        <Outlet />
      </div>
    </div>
  );
}
