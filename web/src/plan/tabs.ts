import { createElement, type JSX } from "react";
import { matchPath } from "react-router";

import { overviewKey, pageHref } from "@/plan/api/page-key";
import { FileMode } from "@/plan/files/file-mode";

export type PlanTab = {
  id: string;
  label: string;
  path: string | null;
  /** Absent when the tab's routes are registered in `router.tsx`. */
  element?: () => JSX.Element;
};

/** Concept targets are resolved against the feature shown by the header. */
export const PLAN_TABS: PlanTab[] = [
  { id: "product", label: "Product", path: null },
  { id: "engineering", label: "Engineering", path: null },
  { id: "files", label: "Files", path: "/files", element: FileMode },
];

export const PLAN_TAB_ROUTES = PLAN_TABS.flatMap(({ element, path }) =>
  element && path ? [{ path: `${path}/*`, element: createElement(element) }] : [],
);

export function planTabPath(tab: PlanTab, feature: string | null): string | null {
  if (tab.id === "product" || tab.id === "engineering") {
    return feature === null ? null : pageHref(overviewKey(feature, tab.id));
  }
  return tab.path;
}

export function isPlanTabActive(tab: PlanTab, pathname: string): boolean {
  if (tab.id === "product" || tab.id === "engineering") {
    return (
      matchPath(`/f/:feature/${tab.id}/*`, pathname) !== null ||
      (tab.id === "engineering" && matchPath("/plans/*", pathname) !== null)
    );
  }
  return tab.path !== null && matchPath(`${tab.path}/*`, pathname) !== null;
}
