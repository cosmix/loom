import { useAtomValue } from "jotai/react";
import { Link, NavLink, useLocation, useMatch } from "react-router";

import { cn } from "cn";
import { featureOfKey, pageKeyOfPath } from "@/plan/api/page-key";
import { activeFeatureAtom, featuresAtom } from "@/plan/state/atoms";
import { isPlanTabActive, PLAN_TABS, planTabPath, type PlanTab } from "@/plan/tabs";
import { VIEW_GLYPHS, type ViewId } from "@/plan/ui/glyphs";

const HomeIcon = VIEW_GLYPHS.home;

function isViewId(id: string): id is ViewId {
  return Object.hasOwn(VIEW_GLYPHS, id);
}

function ViewTab({
  tab,
  feature,
  pathname,
}: {
  tab: PlanTab;
  feature: string | null;
  pathname: string;
}) {
  const Icon = isViewId(tab.id) ? VIEW_GLYPHS[tab.id] : undefined;
  const path = planTabPath(tab, feature);
  const active = isPlanTabActive(tab, pathname);
  const props = {
    className: cn("view-tab", active && "is-active"),
    "aria-current": active ? ("page" as const) : undefined,
    children: (
      <>
        {Icon && <Icon className="size-3.5" aria-hidden="true" />}
        <span className="max-sm:sr-only">{tab.label}</span>
      </>
    ),
  };
  return path === null ? (
    <span {...props} role="link" aria-disabled="true" tabIndex={-1} title="Open a feature first" />
  ) : (
    <Link {...props} to={path} />
  );
}

/**
 * Home and registered planning views, as a segmented control. `view-switch` keeps the dashboard's
 * touch sizing; `plan-view-switch` (header.css) restyles it for the planner.
 */
export function Tabs({ tabs = PLAN_TABS }: { tabs?: PlanTab[] }) {
  const { pathname } = useLocation();
  const featureRoute = useMatch("/f/:feature");
  const activeFeature = useAtomValue(activeFeatureAtom);
  const features = useAtomValue(featuresAtom);
  const pageKey = pageKeyOfPath(pathname);
  const feature =
    (pageKey === null ? null : featureOfKey(pageKey)) ??
    featureRoute?.params.feature ??
    activeFeature ??
    (features.length === 1 ? features[0]?.feature : null) ??
    null;
  return (
    <nav aria-label="Views" className="view-switch plan-view-switch">
      <NavLink to="/" end className={({ isActive }) => cn("view-tab", isActive && "is-active")}>
        <HomeIcon className="size-3.5" aria-hidden="true" />
        <span className="max-sm:sr-only">Home</span>
      </NavLink>
      {tabs.map((tab) => (
        <ViewTab key={tab.id} tab={tab} feature={feature} pathname={pathname} />
      ))}
    </nav>
  );
}
