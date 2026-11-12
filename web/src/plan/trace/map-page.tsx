import type { ReactNode } from "react";

import { metaplanKey, overviewKey, pageHref } from "@/plan/api/page-key";
import { featureCrumb } from "@/plan/files/heading-parts";
import { Breadcrumbs } from "@/plan/ui/doc-heading";

import "@/plan/feature/pages.css";

/**
 * Where each map page sits in its feature: Coverage under the Product spec, Findings under the
 * Plans home (the metaplan page), which holds the run.
 */
const AREAS = {
  coverage: { label: "Product spec", home: (feature: string) => overviewKey(feature) },
  findings: { label: "Plans", home: metaplanKey },
} as const satisfies Record<string, { label: string; home: (feature: string) => string }>;

/**
 * The frame of a feature's map pages (Coverage, Findings): the breadcrumb a document's heading
 * draws ("feat-planner › Product spec › Coverage"), the page title and the standfirst, then the
 * page's content. The look is `maps.css`.
 */
export function MapPage(props: {
  feature: string;
  page: keyof typeof AREAS;
  title: string;
  lede: string;
  children: ReactNode;
}) {
  const { feature, page, title, lede, children } = props;
  const area = AREAS[page];
  const crumbs = [
    featureCrumb(feature),
    { label: area.label, to: pageHref(area.home(feature)) },
    { label: title },
  ];
  return (
    <div className="plan-page">
      <header className="plan-page__head">
        <Breadcrumbs crumbs={crumbs} />
        <h1 className="plan-page-title">{title}</h1>
        <p className="plan-page__lede">{lede}</p>
      </header>
      {children}
    </div>
  );
}
