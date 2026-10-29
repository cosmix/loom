import { createBrowserRouter, type RouteObject } from "react-router";

import { ConceptRoute } from "@/plan/concept/concept-route";
import { FeatureRoute } from "@/plan/feature/feature-route";
import { HomeRoute } from "@/plan/home/home";
import { KickoffRoute } from "@/plan/new/kickoff-route";
import { PlanShell } from "@/plan/shell";
import { PLAN_TAB_ROUTES } from "@/plan/tabs";
import { RouteError } from "@/routes/error";

export const routes: RouteObject[] = [
  {
    path: "/",
    element: <PlanShell />,
    errorElement: <RouteError appName="planner" />,
    children: [
      { index: true, element: <HomeRoute /> },
      { path: "f/:feature", element: <FeatureRoute /> },
      { path: "new", element: <KickoffRoute /> },
      ...["product", "engineering"].flatMap((tab) => [
        { path: `f/:feature/${tab}`, element: <ConceptRoute /> },
        { path: `f/:feature/${tab}/:doc`, element: <ConceptRoute /> },
      ]),
      { path: "plans/:doc", element: <ConceptRoute /> },
      ...PLAN_TAB_ROUTES,
    ],
  },
];

export const router = createBrowserRouter(routes);
