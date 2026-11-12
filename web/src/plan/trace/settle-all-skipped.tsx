import type { JSX } from "react";
import { Link } from "react-router";

import type { SettleAllResult } from "@/plan/api/schema-trace";
import { Untrusted } from "@/plan/chat/visible-controls";
import { featureSourceHref } from "@/plan/feature/source-href";

import { docOfPath, summaryHref } from "./model";
import { useTrace } from "./state";

import "./trace.css";

type Skipped = SettleAllResult["skipped"][number];

/** What the reader can do about a document the bulk settle left alone, by the server's reason. */
const REASON_WORDS: Record<Skipped["reason"], string> = {
  unresolved: "has unresolved points; Settle anyway settles it as it is.",
  // `stale` also covers a plan the server refused for validation errors, which a reload cannot fix.
  stale: "changed or could not be saved; open it to see why.",
  stub: "is still a stub; write it first.",
};

/** The documents a bulk settle skipped, each a link to the document with the reason beside it. */
export function SkippedDocuments({
  feature,
  skipped,
}: {
  feature: string;
  skipped: Skipped[];
}): JSX.Element {
  const { trace } = useTrace(feature);
  return (
    <ul aria-label="Documents not settled" className="trace-settle-skipped">
      {skipped.map(({ path, reason }) => {
        const doc = trace ? docOfPath(trace, path) : undefined;
        const to =
          doc && doc.progress !== "stub"
            ? summaryHref(feature, doc)
            : featureSourceHref(feature, path);
        return (
          <li key={path}>
            <Link to={to}>
              <Untrusted text={doc?.title ?? path} />
            </Link>{" "}
            {REASON_WORDS[reason]}
          </li>
        );
      })}
    </ul>
  );
}
