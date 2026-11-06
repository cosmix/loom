import { useAtomValue } from "jotai/react";
import type { JSX } from "react";

import type { TreeEntry } from "@/plan/api/schema-files";
import { visibleControls } from "@/plan/chat/visible-controls";
import { SentenceText } from "@/plan/concept/sentence-text";
import { AttentionBar } from "@/plan/files/attention-bar";
import { questionDocs } from "@/plan/files/rail-model";
import { TreeEntryRow } from "@/plan/files/tree-entry";
import { openQuestionsAtom } from "@/plan/review/live";

/** "11 sections · 2 missing": what a feature's product spec holds, its gaps counted apart. */
function productCount(feature: TreeEntry): string {
  const sections = feature.children.filter((child) => child.kind === "section").length;
  const missing = feature.children.filter((child) => child.gap).length;
  const parts = [`${sections} ${sections === 1 ? "section" : "sections"}`];
  if (missing > 0) parts.push(`${missing} missing`);
  return parts.join(" · ");
}

function Rows({ entries }: { entries: TreeEntry[] }): JSX.Element {
  return (
    <ul className="plan-tree__list plan-product__rows">
      {entries.map((entry) => (
        <TreeEntryRow key={entry.path ?? entry.title} entry={entry} forceOpen={false} />
      ))}
    </ul>
  );
}

function FeatureProduct({ feature }: { feature: TreeEntry }): JSX.Element {
  const counts = useAtomValue(openQuestionsAtom);
  const name = feature.feature ?? feature.title;
  return (
    <section aria-label={visibleControls(name)} className="plan-rail">
      <div className="plan-rail__head">
        <div className="plan-rail__heading">
          <span className="plan-rail__name">
            <SentenceText text={name} />
          </span>
          <span className="plan-rail__complete">{productCount(feature)}</span>
        </div>
      </div>
      <AttentionBar docs={questionDocs(feature.children, counts)} />
      <Rows entries={feature.children} />
    </section>
  );
}

/**
 * The Product tab: each feature's vision and sections under its name, as rows that open the
 * document (or a missing one's gap page), then any product document outside a feature.
 */
export function ProductTab({ entries }: { entries: TreeEntry[] }): JSX.Element {
  const features = entries.filter((entry) => entry.kind === "feature");
  const loose = entries.filter((entry) => entry.kind !== "feature");
  if (entries.length === 0) {
    return <p className="plan-empty plan-rail__empty">No product documents yet.</p>;
  }
  return (
    <div className="plan-rail-tab plan-product">
      {features.map((feature) => (
        <FeatureProduct key={feature.title} feature={feature} />
      ))}
      {loose.length > 0 && <Rows entries={loose} />}
    </div>
  );
}
