import { createElement, type JSX } from "react";

export type PlanTab = {
  id: string;
  label: string;
  path: string;
  element: () => JSX.Element;
};

/** Plan 3 adds Files; plan 5 adds Product and Engineering. */
export const PLAN_TABS: PlanTab[] = [];

export const PLAN_TAB_ROUTES = PLAN_TABS.map((tab) => ({
  path: `${tab.path}/*`,
  element: createElement(tab.element),
}));
