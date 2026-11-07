import type { JSX } from "react";
import { Link } from "react-router";

import { isPlanFile } from "@/plan/chat/turn-footer-plans";
import { Untrusted } from "@/plan/chat/visible-controls";
import { featureOfDocPath, featureSourceHref } from "@/plan/feature/source-href";

import { settleWords } from "./doc-model";
import { docOfPath, impactFor, summaryHref } from "./model";
import { SettleGlyph } from "./settle-glyph";
import { useTrace } from "./state";

import "./trace.css";

/**
 * Under a changed document in the turn footer, "Now out of step with what you settled": the
 * documents that moved with it (`revised`, `stub_changed`, `upstream_changed`) and the plans
 * covering a changed section, each with its settle glyph and state unless it is done work. Titles
 * come from the documents, so they are drawn as text. Nothing renders for a document outside a
 * feature or one with no downstream. `omitPlans` names plan slugs whose rows are left out because
 * the caller draws those plans itself.
 */
export function ImpactRows({
  path,
  omitPlans,
}: {
  path: string;
  omitPlans?: ReadonlySet<string>;
}): JSX.Element | null {
  const feature = featureOfDocPath(path);
  const { trace } = useTrace(feature ?? "");
  if (feature === null || !trace) return null;
  const rows = (impactFor(trace, [path])[0]?.rows ?? []).filter(
    (row) => !omitPlans || ![...omitPlans].some((slug) => isPlanFile(row.path, slug)),
  );
  if (rows.length === 0) return null;
  return (
    <div className="trace-impact">
      <p className="trace-impact__head">Now out of step with what you settled</p>
      <ul aria-label="Affected documents" className="trace-impact__list">
        {rows.map((row) => {
          const doc = docOfPath(trace, row.path);
          const to = doc ? summaryHref(feature, doc) : featureSourceHref(feature, row.path);
          return (
            <li key={row.path} className="trace-impact__row" data-settle={row.state ?? undefined}>
              {row.state !== null && <SettleGlyph state={row.state} />}
              <span className="trace-impact__text">
                <Link to={to}>
                  <Untrusted text={row.title} />
                </Link>
                {row.state !== null && doc && (
                  <>
                    {/* The space keeps the title and its state apart as text. */}{" "}
                    <span className="trace-impact__state">
                      {settleWords(row.state, doc.role).chip}
                    </span>
                  </>
                )}
              </span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
