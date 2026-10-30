import { createBrowserRouter, type RouteObject } from "react-router";

import { ConceptRoute } from "@/plan/concept/concept-route";
import { MetaplanPage, VisionPage } from "@/plan/feature/document-page";
import { FeatureLanding, FeatureLayout } from "@/plan/feature/feature-layout";
import { Gaps } from "@/plan/feature/gaps";
import { HomeRoute } from "@/plan/home/home";
import { KickoffRoute } from "@/plan/new/kickoff-route";
import { PlanShell } from "@/plan/shell";
import { PLAN_TAB_ROUTES } from "@/plan/tabs";
import { LoadState } from "@/plan/ui/load-state";
import { RouteError } from "@/routes/error";

/** A feature's overview and its document summaries, with the overview carrying the gaps. */
function conceptRoutes(scope: "product" | "engineering"): RouteObject[] {
  return [
    { path: scope, element: <ConceptRoute above={<Gaps scope={scope} />} /> },
    { path: `${scope}/:doc`, element: <ConceptRoute /> },
  ];
}

const featureRoutes: RouteObject = {
  path: "f/:feature",
  element: <FeatureLayout />,
  children: [
    { index: true, element: <FeatureLanding /> },
    { path: "vision", element: <VisionPage /> },
    ...conceptRoutes("product"),
    { path: "metaplan", element: <MetaplanPage /> },
    { path: "metaplan/:file", element: <MetaplanPage /> },
    ...conceptRoutes("engineering"),
    // The spine links to Pressure test and Run; their pages are routed ahead of this one when
    // they exist, and until then a link to either lands on an honest error, not a blank page.
    { path: "*", element: <LoadState state="error" title="No such page" /> },
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
      { path: "plans/:doc", element: <ConceptRoute /> },
      ...PLAN_TAB_ROUTES,
    ],
  },
];

export const router = createBrowserRouter(routes);
