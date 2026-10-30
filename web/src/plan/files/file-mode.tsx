import { useAtomValue, useStore } from "jotai/react";
import type { JSX } from "react";
import { useLocation } from "react-router";

import { useMediaQuery } from "@/lib/use-media-query";
import { useFileSocket } from "@/plan/files/api";
import { DiffPage } from "@/plan/diff";
import { parseAddress, type FileAddress } from "@/plan/files/address";
import { treeAtom } from "@/plan/files/atoms";
import { featureCrumb } from "@/plan/files/heading-parts";
import { ReadingColumn } from "@/plan/files/reading-column";
import { Tree } from "@/plan/files/tree";
import { DrawerOpener, TreeDrawer, TreeDrawerRoot, useTreeDrawer } from "@/plan/files/tree-drawer";
import { SentenceText } from "@/plan/concept/sentence-text";
import { OfferButton } from "@/plan/offers/offer-button";
import { featuresAtom } from "@/plan/state/atoms";
import { DocHeading } from "@/plan/ui/doc-heading";
import { LoadState } from "@/plan/ui/load-state";

/** Wide enough for the 18rem tree beside a reading column. */
const WIDE = "(min-width: 900px)";

/** A missing document: the checklist item that asks for it, with its offer. */
function GapView({
  title,
  feature,
  target,
}: {
  title: string;
  feature: string | null;
  target: string | null;
}): JSX.Element {
  const features = useAtomValue(featuresAtom);
  const crumbs = feature !== null ? [featureCrumb(feature)] : [];
  const item = features
    .find((checklist) => checklist.feature === feature)
    ?.items.find((candidate) => target !== null && candidate.path === target);
  return (
    <div className="flex flex-col gap-3">
      <DocHeading breadcrumb={[...crumbs, { label: title }]} kind={null} title={title} />
      <p className="px-6 text-[13px] text-muted-foreground">This document is missing.</p>
      {item ? (
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 px-6 pb-6 text-[13px]">
          <span className="text-muted-foreground">Step {item.step}</span>
          <span className="font-medium" style={{ unicodeBidi: "isolate", direction: "ltr" }}>
            <SentenceText text={item.text} />
          </span>
          {item.offer && <OfferButton offer={item.offer} />}
        </div>
      ) : (
        <p className="px-6 pb-6 text-[13px] text-muted-foreground">
          No checklist item asks for it yet.
        </p>
      )}
    </div>
  );
}

/** No document open: the drawer control on narrow screens, then the gap view or a hint. */
function NoDocument({ search, opener }: { search: URLSearchParams; opener?: JSX.Element }) {
  const gap = search.get("gap");
  return (
    <>
      {opener}
      {gap ? (
        <GapView title={gap} feature={search.get("feature")} target={search.get("target")} />
      ) : (
        <p className="p-6 text-[13px] text-muted-foreground">Choose a document from the tree.</p>
      )}
    </>
  );
}

/** What the reading region holds: a diff, a document, or the no-document state. */
function DocumentPane({
  address,
  hasPath,
  search,
  opener,
}: {
  address: FileAddress;
  hasPath: boolean;
  search: URLSearchParams;
  opener?: JSX.Element;
}): JSX.Element {
  const tree = useAtomValue(treeAtom);
  if (!hasPath) return <NoDocument search={search} opener={opener} />;
  if (address.diff) return <DiffPage path={address.path} kind={address.diff} turn={address.turn} />;
  return <ReadingColumn path={address.path} anchor={address.anchor} tree={tree} opener={opener} />;
}

/** The Files tab: the tree beside the reading column, a drawer on narrow screens. */
export function FileMode(): JSX.Element {
  const store = useStore();
  useFileSocket(store);
  const tree = useAtomValue(treeAtom);
  const location = useLocation();
  const wide = useMediaQuery(WIDE);
  const drawer = useTreeDrawer();
  const address = parseAddress(location);
  const hasPath = location.pathname.startsWith("/files/") && location.pathname.length > 7;
  const search = new URLSearchParams(location.search);
  const opener = wide ? undefined : <DrawerOpener />;

  const tab = tree ? (
    <Tree tree={tree} open={hasPath ? address.path : null} />
  ) : (
    <LoadState state="loading" title="Loading the documents" />
  );

  return (
    <TreeDrawerRoot drawer={drawer}>
      <div className="flex h-full min-h-0">
        {wide ? (
          <aside className="flex w-72 shrink-0 flex-col border-r border-hairline">{tab}</aside>
        ) : (
          <TreeDrawer drawer={drawer}>{tab}</TreeDrawer>
        )}
        <div
          ref={drawer.reading}
          data-reading-scroll
          role="region"
          aria-label="Document"
          tabIndex={-1}
          className="min-w-0 flex-1 overflow-y-auto outline-none"
        >
          <DocumentPane address={address} hasPath={hasPath} search={search} opener={opener} />
        </div>
      </div>
    </TreeDrawerRoot>
  );
}
