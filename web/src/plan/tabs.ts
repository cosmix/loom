import { createElement, type JSX } from "react";

import { FileMode } from "@/plan/files/file-mode";

export type PlanTab = {
  id: string;
  label: string;
  path: string;
  element: () => JSX.Element;
};

/** Plan 3 adds Files; plan 5 adds Product and Engineering. */
export const PLAN_TABS: PlanTab[] = [
  { id: "files", label: "Files", path: "/files", element: FileMode },
];

export const PLAN_TAB_ROUTES = PLAN_TABS.map((tab) => ({
  path: `${tab.path}/*`,
  element: createElement(tab.element),
}));
