import { useAtomValue, useStore } from "jotai/react";
import type { JSX } from "react";
import { useLocation } from "react-router";

import { useMediaQuery } from "@/lib/use-media-query";
import { useFileSocket } from "@/plan/files/api";
import { DiffPage } from "@/plan/diff";
import { parseAddress } from "@/plan/files/address";
import { treeAtom } from "@/plan/files/atoms";
import { ReadingColumn } from "@/plan/files/reading-column";
import { Tree } from "@/plan/files/tree";
import { DrawerOpener, TreeDrawer, TreeDrawerRoot, useTreeDrawer } from "@/plan/files/tree-drawer";
import { OfferButton } from "@/plan/offers/offer-button";
import { checklistAtom } from "@/plan/state/atoms";
import { LoadState } from "@/plan/ui/load-state";

/** Wide enough for the 18rem tree beside a reading column. */
const WIDE = "(min-width: 900px)";

/** A missing document: the checklist item that asks for it, with its offer. */
function GapView({ title }: { title: string }): JSX.Element {
  const checklist = useAtomValue(checklistAtom);
  const needle = title.toLowerCase();
  const item = checklist?.items.find((candidate) => candidate.text.toLowerCase().includes(needle));
  return (
    <div className="flex flex-col gap-3 p-6">
      <h1 className="font-display text-2xl">{title}</h1>
      <p className="text-[13px] text-muted-foreground">This document is missing.</p>
      {item ? (
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[13px]">
          <span className="text-muted-foreground">Step {item.step}</span>
          <span className="font-medium">{item.text}</span>
          {item.offer && <OfferButton offer={item.offer} />}
        </div>
      ) : (
        <p className="text-[13px] text-muted-foreground">No checklist item asks for it yet.</p>
      )}
    </div>
  );
}

/** No document open: the drawer control on narrow screens, then the gap view or a hint. */
function NoDocument({ gap, opener }: { gap: string | null; opener?: JSX.Element }) {
  return (
    <>
      {opener}
      {gap ? (
        <GapView title={gap} />
      ) : (
        <p className="p-6 text-[13px] text-muted-foreground">Choose a document from the tree.</p>
      )}
    </>
  );
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
  const gap = new URLSearchParams(location.search).get("gap");
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
          {hasPath && address.diff ? (
            <DiffPage path={address.path} kind={address.diff} turn={address.turn} />
          ) : hasPath ? (
            <ReadingColumn
              path={address.path}
              anchor={address.anchor}
              tree={tree}
              opener={opener}
            />
          ) : (
            <NoDocument gap={gap} opener={opener} />
          )}
        </div>
      </div>
    </TreeDrawerRoot>
  );
}
