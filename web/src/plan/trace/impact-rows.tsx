import type { JSX } from "react";
import { Link } from "react-router";

import { Untrusted } from "@/plan/chat/visible-controls";
import { featureOfDocPath, featureSourceHref } from "@/plan/feature/source-href";

import { docOfPath, impactFor, settleLabel, summaryHref } from "./model";
import { useTrace } from "./state";

import "./trace.css";

/**
 * Under a changed document in the turn footer: the documents that moved with it (`revised`,
 * `stub_changed`, `upstream_changed`) and the plans covering a changed section, each with its
 * settle state. Titles come from the documents, so they are drawn as text. Nothing renders for
 * a document outside a feature or one with no downstream.
 */
export function ImpactRows({ path }: { path: string }): JSX.Element | null {
  const feature = featureOfDocPath(path);
  const { trace } = useTrace(feature ?? "");
  if (feature === null || !trace) return null;
  const rows = impactFor(trace, [path])[0]?.rows ?? [];
  if (rows.length === 0) return null;
  return (
    <>
      {/* The space keeps the rows apart from the file chip beside them as text. */}{" "}
      <ul className="trace-impact" aria-label="Affected documents">
        {rows.map((row) => {
          const doc = docOfPath(trace, row.path);
          const to = doc ? summaryHref(feature, doc) : featureSourceHref(feature, row.path);
          return (
            <li key={row.path}>
              <Link to={to}>
                <Untrusted text={row.title} />
              </Link>{" "}
              <span className="trace-impact-state" data-settle={row.state}>
                {settleLabel(row.state)}
              </span>
            </li>
          );
        })}
      </ul>
    </>
  );
}
