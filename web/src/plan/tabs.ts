import { createElement, type JSX } from "react";
import { matchPath } from "react-router";

import { FileMode } from "@/plan/files/file-mode";

export type PlanTab = {
  id: string;
  label: string;
  path: string;
  /** The route the tab opens; `PLAN_TAB_ROUTES` registers it under `path`. */
  element: () => JSX.Element;
};

/**
 * The views the header links to beside Home. A feature's documents are reached through the spine
 * (`feature/spine.tsx`), so no feature page has a tab here.
 */
export const PLAN_TABS: PlanTab[] = [
  { id: "files", label: "Files", path: "/files", element: FileMode },
];

export const PLAN_TAB_ROUTES = PLAN_TABS.map(({ element, path }) => ({
  path: `${path}/*`,
  element: createElement(element),
}));

export function isPlanTabActive(tab: PlanTab, pathname: string): boolean {
  return matchPath(`${tab.path}/*`, pathname) !== null;
}
