import { useAtomValue, useStore } from "jotai/react";
import { useEffect, useState, type JSX } from "react";
import { useLocation } from "react-router";

import { Button } from "@/components/ui/button";
import { useMediaQuery } from "@/lib/use-media-query";
import { fetchTree, useFileSocket } from "@/plan/files/api";
import { parseAddress } from "@/plan/files/address";
import { treeAtom } from "@/plan/files/atoms";
import { ReadingColumn } from "@/plan/files/reading-column";
import { Tree } from "@/plan/files/tree";
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

function Drawer({ onClose, children }: { onClose: () => void; children: JSX.Element }) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);
  return (
    <>
      <button
        type="button"
        aria-label="Close the document list"
        className="fixed inset-0 z-30 bg-black/20"
        onClick={onClose}
      />
      <div
        role="dialog"
        aria-label="Documents"
        className="fixed inset-y-0 left-0 z-40 flex w-72 max-w-[85vw] flex-col border-r border-hairline bg-background"
      >
        {children}
      </div>
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
  const [drawer, setDrawer] = useState(false);
  const address = parseAddress(location);
  const hasPath = location.pathname.startsWith("/files/") && location.pathname.length > 7;
  const gap = new URLSearchParams(location.search).get("gap");

  useEffect(() => {
    if (store.get(treeAtom)) return;
    void fetchTree().then((result) => {
      if (result.ok && !store.get(treeAtom)) store.set(treeAtom, result.value);
    });
  }, [store]);
  useEffect(() => setDrawer(false), [location.pathname, location.search]);

  const tab = tree ? (
    <Tree tree={tree} open={hasPath ? address.path : null} />
  ) : (
    <LoadState state="loading" title="Loading the documents" />
  );

  return (
    <div className="flex h-full min-h-0">
      {wide && (
        <aside className="flex w-72 shrink-0 flex-col border-r border-hairline">{tab}</aside>
      )}
      {!wide && drawer && <Drawer onClose={() => setDrawer(false)}>{tab}</Drawer>}
      <div data-reading-scroll className="min-w-0 flex-1 overflow-y-auto">
        {hasPath ? (
          <ReadingColumn
            path={address.path}
            anchor={address.anchor}
            tree={tree}
            onOpenTree={wide ? undefined : () => setDrawer(true)}
          />
        ) : (
          <>
            {!wide && (
              <div className="border-b border-hairline px-4 py-2">
                <Button variant="ghost" size="sm" onClick={() => setDrawer(true)}>
                  Documents
                </Button>
              </div>
            )}
            {gap ? (
              <GapView title={gap} />
            ) : (
              <p className="p-6 text-[13px] text-muted-foreground">
                Choose a document from the tree.
              </p>
            )}
          </>
        )}
      </div>
    </div>
  );
}
