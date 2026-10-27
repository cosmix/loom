import { createBrowserRouter, type RouteObject } from "react-router";

import { ChecklistView } from "@/plan/checklist-view";
import { ConceptRoute } from "@/plan/concept/concept-route";
import { PlanShell } from "@/plan/shell";
import { PLAN_TAB_ROUTES } from "@/plan/tabs";
import { RouteError } from "@/routes/error";

export const routes: RouteObject[] = [
  {
    path: "/",
    element: <PlanShell />,
    errorElement: <RouteError />,
    children: [
      { index: true, element: <ChecklistView /> },
      ...["product", "engineering"].flatMap((tab) => [
        { path: tab, element: <ConceptRoute /> },
        { path: `${tab}/:doc`, element: <ConceptRoute /> },
      ]),
      ...PLAN_TAB_ROUTES,
    ],
  },
];

export const router = createBrowserRouter(routes);
