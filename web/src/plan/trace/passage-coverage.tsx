import { Fragment, type JSX } from "react";
import { Link } from "react-router";

import { Untrusted } from "@/plan/chat/visible-controls";
import { featureOfDocPath } from "@/plan/feature/source-href";
import { featureHref } from "@/plan/feature/use-feature";
import { plural } from "@/plan/ui/plural";

import { coverageOfHeading, type CoveringPlan } from "./doc-model";
import { coverageHref } from "./model";
import { useTrace } from "./state";

import "./trace.css";

function Plans({ plans }: { plans: CoveringPlan[] }): JSX.Element {
  return (
    <>
      Covered by{" "}
      {plans.map((plan, index) => (
        <Fragment key={plan.slug}>
          {index > 0 && ", "}
          <Link to={plan.to}>
            <code className="concept-passage-coverage__slug">
              <Untrusted text={plan.slug} />
            </code>
          </Link>
          {plan.stages.length > 0 && (
            <>
              {" › "}
              <Untrusted text={plan.stages.join(", ")} />
            </>
          )}
        </Fragment>
      ))}
    </>
  );
}

/**
 * The line an opened passage ends with: the plans and stages covering the spec heading it quotes,
 * and its open findings ("Covered by 03-summaries › fragments · 1 open finding"), or "Not covered
 * by any plan" linking to the heading on the Coverage page. Nothing renders for a passage the trace
 * has no heading for (a file outside the spec, or a whole-document passage).
 */
export function PassageCoverage(props: { path: string; anchor: string }): JSX.Element | null {
  const { path, anchor } = props;
  const feature = featureOfDocPath(path);
  const { trace } = useTrace(feature ?? "");
  const coverage = trace ? coverageOfHeading(trace, path, anchor) : null;
  if (feature === null || !coverage) return null;
  return (
    <p className="concept-passage-coverage" data-covered={coverage.plans.length > 0}>
      {coverage.plans.length > 0 ? (
        <Plans plans={coverage.plans} />
      ) : (
        <Link to={coverageHref(feature, path, anchor)}>Not covered by any plan</Link>
      )}
      {coverage.open > 0 && (
        <>
          {" · "}
          <Link to={featureHref(feature, "findings")}>{plural(coverage.open, "open finding")}</Link>
        </>
      )}
    </p>
  );
}
