import { useEffect, useState, type JSX } from "react";

import type { FileTree } from "@/plan/api/schema-files";
import { PlansTab } from "@/plan/files/plan-rail";
import { ProductTab } from "@/plan/files/product-list";
import { containsPath } from "@/plan/files/tree-entry";

import "./rail.css";

const TABS = [
  ["product", "Product"],
  ["plans", "Plans"],
] as const;

type Tab = (typeof TABS)[number][0];

/** The tab listing the open document, or null when neither does. */
function tabOf(tree: FileTree, open: string | null): Tab | null {
  if (tree.product.some((entry) => containsPath(entry, open))) return "product";
  if (tree.plans.some((entry) => containsPath(entry, open))) return "plans";
  return null;
}

/**
 * The tree's two working lists behind a segmented switch: the product spec and the plans. Plans
 * is chosen first; opening a document moves the switch to the tab that lists it.
 */
export function TreeTabs({ tree, open }: { tree: FileTree; open: string | null }): JSX.Element {
  const holder = tabOf(tree, open);
  const [tab, setTab] = useState<Tab>(holder ?? "plans");
  useEffect(() => {
    if (holder !== null) setTab(holder);
  }, [holder, open]);
  return (
    <>
      <div role="group" aria-label="List" className="plan-tabs">
        {TABS.map(([id, label]) => (
          <button
            key={id}
            type="button"
            aria-pressed={tab === id}
            className="plan-tabs__tab"
            onClick={() => setTab(id)}
          >
            {label}
          </button>
        ))}
      </div>
      {tab === "plans" ? <PlansTab entries={tree.plans} /> : <ProductTab entries={tree.product} />}
    </>
  );
}
