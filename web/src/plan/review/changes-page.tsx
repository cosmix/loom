import { useAtomValue } from "jotai";
import { useEffect, useRef } from "react";
import { Link, useLocation } from "react-router";

import { serverInfoAtom } from "@/plan/state/atoms";
import { LoadState } from "@/plan/ui/load-state";

import "./changed-since.css";
import { ComparisonList, changesFrom, useChangedSince } from "./changed-since";
import { checkpointAtom } from "./checkpoint";
import { hashesAtom } from "./live";

/**
 * Nothing left to review, and the way back to where the reader came from (else Home). When Mark
 * all seen empties the list, its button goes with it; the way back takes the focus it held, so
 * the focus does not drop to the page body.
 */
function NothingChanged({ from }: { from: string | null }) {
  const back = useRef<HTMLAnchorElement>(null);
  useEffect(() => {
    const active = document.activeElement;
    if (!active || active === document.body || !active.isConnected) back.current?.focus();
  }, []);
  return (
    <div className="changes-page__empty">
      <p className="plan-empty">Nothing changed since you looked.</p>
      <Link ref={back} to={from ?? "/"} className="changed-since__link">
        {from ? "Back to where you were" : "Go to Home"}
      </Link>
    </div>
  );
}

/** The comparison once the checkpoint and the live hashes are known, keyed by project. */
function ChangesBody({ from }: { from: string | null }) {
  const changes = useChangedSince();
  const projectKey = useAtomValue(serverInfoAtom)?.project_key;
  const checkpoint = useAtomValue(checkpointAtom);
  const hashes = useAtomValue(hashesAtom);
  if (!projectKey || !checkpoint || !hashes) {
    return <LoadState state="loading" title="Loading changes" />;
  }
  return (
    <ComparisonList key={projectKey} changes={changes} empty={<NothingChanged from={from} />} />
  );
}

/**
 * "Changed since you looked": the documents edited since the reader last viewed them, across the
 * page's width so a side-by-side diff has room. Arriving (from the activity popover or a link)
 * puts focus on the heading, so a screen reader starts at the page's title.
 */
export function ChangesPage() {
  const location = useLocation();
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    heading.current?.focus({ preventScroll: true });
  }, [location.key]);
  return (
    <div className="changes-page">
      <header className="changes-page__head">
        <p className="plan-eyebrow">Review</p>
        <h1 ref={heading} tabIndex={-1} className="plan-page-title">
          Changed since you looked
        </h1>
        <p className="changes-page__lede">
          Documents that changed since you last looked at them in this browser.
        </p>
        <p className="plan-fine">
          Opening a document’s changes marks it seen. Mark all seen starts over from now.
        </p>
      </header>
      <ChangesBody from={changesFrom(location.state)} />
    </div>
  );
}
