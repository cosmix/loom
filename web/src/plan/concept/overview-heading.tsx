import { useAtomValue } from "jotai/react";
import type { JSX, ReactNode } from "react";

import { docKindOfPageKey, featureOfKey } from "@/plan/api/page-key";
import type { ConceptPage } from "@/plan/api/schema-concept";
import { featureCrumb, isRebuilding } from "@/plan/files/heading-parts";
import { featuresAtom } from "@/plan/state/atoms";
import { DocHeading, type HeadingCrumb, ToolbarGroup } from "@/plan/ui/doc-heading";

import { DocumentKey } from "./document-key";
import { LedgerRows } from "./header-ledger";
import { hasOpenable } from "./open-all";
import { OpenAllButton } from "./page-heading";
import { OverviewSummariesRow } from "./overview-summaries";

/** Feature slug, the Product spec area, then the overview; the slug only for a known feature. */
function useOverviewCrumbs(pageKey: string): HeadingCrumb[] {
  const feature = featureOfKey(pageKey);
  const known = useAtomValue(featuresAtom).some((item) => item.feature === feature);
  const trail: HeadingCrumb[] = [{ label: "Product spec" }, { label: "Overview" }];
  return feature !== null && known ? [featureCrumb(feature), ...trail] : trail;
}

/**
 * An overview's heading, the document step of the summary pages (ui/DESIGN.md, Page rhythm): the
 * breadcrumb, the title with its mark, then the ledger (the summaries' state with its rebuild
 * question, the caller's `ledger` rows, the sources) and the view bar with Open all while the page
 * has a sentence that opens.
 */
export function OverviewHeading({
  page,
  pageKey,
  ledger,
}: {
  page: ConceptPage;
  pageKey: string;
  ledger: ReactNode;
}): JSX.Element {
  const crumbs = useOverviewCrumbs(pageKey);
  return (
    <DocHeading
      breadcrumb={crumbs}
      kind={docKindOfPageKey(page.key)}
      title="Product overview"
      rebuilding={isRebuilding(page)}
      actions={<ToolbarGroup>{hasOpenable(page) && <OpenAllButton page={page} />}</ToolbarGroup>}
    >
      <LedgerRows>
        <OverviewSummariesRow page={page} />
        {ledger}
        {page.key_chips.length > 0 && <DocumentKey page={page} pageKey={pageKey} />}
      </LedgerRows>
    </DocHeading>
  );
}
