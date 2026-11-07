import { createBrowserRouter, Navigate, type RouteObject } from "react-router";

import { ConceptRoute } from "@/plan/concept/concept-route";
import { FeatureSourcePage, MetaplanPage, VisionPage } from "@/plan/feature/document-page";
import { FeatureLanding, FeatureLayout } from "@/plan/feature/feature-layout";
import { Gaps } from "@/plan/feature/gaps";
import { RunPanel } from "@/plan/feature/run-panel";
import { HomeRoute } from "@/plan/home/home";
import { KickoffRoute } from "@/plan/new/kickoff-route";
import { ChangesPage } from "@/plan/review/changes-page";
import { PlanShell } from "@/plan/shell";
import { CoveragePage } from "@/plan/trace/coverage-page";
import { FindingsPage } from "@/plan/trace/findings-page";
import { PLAN_TAB_ROUTES } from "@/plan/tabs";
import { LoadState } from "@/plan/ui/load-state";
import { RouteError } from "@/routes/error";

/**
 * A feature's overview and its document summaries, with the overview carrying the gaps. The Plans
 * overview also carries the run: its state, Start a run and the dashboard link.
 */
function conceptRoutes(scope: "product" | "engineering"): RouteObject[] {
  const above = (
    <>
      {scope === "engineering" && <RunPanel />}
      <Gaps scope={scope} />
    </>
  );
  return [
    { path: scope, element: <ConceptRoute above={above} /> },
    { path: `${scope}/:doc`, element: <ConceptRoute /> },
  ];
}

const featureRoutes: RouteObject = {
  path: "f/:feature",
  element: <FeatureLayout />,
  children: [
    { index: true, element: <FeatureLanding /> },
    { path: "vision", element: <VisionPage /> },
    { path: "source/*", element: <FeatureSourcePage /> },
    ...conceptRoutes("product"),
    { path: "metaplan", element: <MetaplanPage /> },
    ...conceptRoutes("engineering"),
    // The run lives on the Plans overview; an old Run link lands there.
    { path: "run", element: <Navigate to="../engineering" replace /> },
    { path: "coverage", element: <CoveragePage /> },
    { path: "findings", element: <FindingsPage /> },
    // Any other child of a feature is a mistyped link: say so rather than draw a blank page.
    { path: "*", element: <LoadState state="not_found" title="No such page" /> },
  ],
};

export const routes: RouteObject[] = [
  {
    path: "/",
    element: <PlanShell />,
    errorElement: <RouteError appName="planner" />,
    children: [
      { index: true, element: <HomeRoute /> },
      featureRoutes,
      { path: "new", element: <KickoffRoute /> },
      { path: "changes", element: <ChangesPage /> },
      { path: "plans/:doc", element: <ConceptRoute /> },
      ...PLAN_TAB_ROUTES,
    ],
  },
];

export const router = createBrowserRouter(routes);
