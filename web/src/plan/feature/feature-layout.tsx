import { useSetAtom } from "jotai/react";
import { useEffect } from "react";
import { Link, Navigate, Outlet } from "react-router";

import { Untrusted } from "@/plan/chat/visible-controls";
import { landingPath } from "@/plan/feature/landing";
import { Spine } from "@/plan/feature/spine";
import { useFeature } from "@/plan/feature/use-feature";
import { activeFeatureAtom } from "@/plan/state/atoms";
import { LoadState } from "@/plan/ui/load-state";

import "./feature.css";

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

/** The index route of `/f/:feature`: the page the feature's state calls for. */
export function FeatureLanding() {
  const { checklist } = useFeature();
  if (!checklist) return <LoadState state="loading" title="Loading the feature" />;
  return <Navigate to={landingPath(checklist)} replace />;
}

/**
 * Every `/f/:feature` page: the spine beside the routed page. Until the project arrives the page
 * renders alone, so a concept page can start loading; the spine joins it once the feature is known.
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
    <div className="plan-layout" data-spine={checklist ? "" : undefined}>
      {checklist && <Spine />}
      <div className="plan-layout__page">
        <Outlet />
      </div>
    </div>
  );
}
