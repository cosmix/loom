import { lazy, Suspense, type ReactNode } from "react";
import { createBrowserRouter, Navigate, type RouteObject } from "react-router";

import { ConceptRoute } from "@/plan/concept/concept-route";
import { FeatureSourcePage, MetaplanPage, VisionPage } from "@/plan/feature/document-page";
import { FeatureLanding, FeatureLayout } from "@/plan/feature/feature-layout";
import { Gaps } from "@/plan/feature/gaps";
import { useFeature } from "@/plan/feature/use-feature";
import { HomeRoute } from "@/plan/home/home";
import { KickoffRoute } from "@/plan/new/kickoff-route";
import { ChangesPage } from "@/plan/review/changes-page";
import { PlanShell } from "@/plan/shell";
import { SettleAllButton } from "@/plan/trace/settle-all";
import { PLAN_TAB_ROUTES } from "@/plan/tabs";
import { LoadState } from "@/plan/ui/load-state";
import { RouteError } from "@/routes/error";

// Coverage and Findings are reached from the spine, never on first paint; each is its own chunk so
// their tables and cards stay out of plan.js (scripts/bundle-rules.json guards the size).
const CoveragePage = lazy(() =>
  import("@/plan/trace/coverage-page").then((module) => ({ default: module.CoveragePage })),
);
const FindingsPage = lazy(() =>
  import("@/plan/trace/findings-page").then((module) => ({ default: module.FindingsPage })),
);

/** The loading state fills the routed page, so the page does not jump when the chunk arrives. */
function Deferred({ children }: { children: ReactNode }) {
  return <Suspense fallback={<LoadState state="loading" />}>{children}</Suspense>;
}

/** The product overview carries the section gaps and the bulk settle action. */
function ProductOverview() {
  const { checklist } = useFeature();
  return (
    <ConceptRoute
      above={
        <>
          <Gaps scope="product" />
          {checklist && <SettleAllButton feature={checklist.feature} role="section" />}
        </>
      }
    />
  );
}

/** Product keeps its overview; the Plans home is the metaplan. Document summaries keep their scope. */
function conceptRoutes(scope: "product" | "engineering"): RouteObject[] {
  return [
    {
      path: scope,
      element: scope === "product" ? <ProductOverview /> : <Navigate to="../metaplan" replace />,
    },
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
    // The metaplan carries the run; an old Run link lands there.
    { path: "run", element: <Navigate to="../metaplan" replace /> },
    {
      path: "coverage",
      element: (
        <Deferred>
          <CoveragePage />
        </Deferred>
      ),
    },
    {
      path: "findings",
      element: (
        <Deferred>
          <FindingsPage />
        </Deferred>
      ),
    },
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
