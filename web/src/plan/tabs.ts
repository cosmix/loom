import { createElement, type JSX } from "react";

import { FileMode } from "@/plan/files/file-mode";

export type PlanTab = {
  id: string;
  label: string;
  path: string;
  /** Absent when the tab's routes are registered in `router.tsx`. */
  element?: () => JSX.Element;
};

/** Plan 3 adds Files; plan 5 adds Product and Engineering. */
export const PLAN_TABS: PlanTab[] = [
  { id: "product", label: "Product", path: "/product" },
  { id: "engineering", label: "Engineering", path: "/engineering" },
  { id: "files", label: "Files", path: "/files", element: FileMode },
];

export const PLAN_TAB_ROUTES = PLAN_TABS.flatMap(({ element, path }) =>
  element ? [{ path: `${path}/*`, element: createElement(element) }] : [],
);
