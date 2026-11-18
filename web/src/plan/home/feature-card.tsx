import { CircleCheckBig } from "lucide-react";
import { useAtomValue } from "jotai/react";
import { useId, useMemo, type ReactNode } from "react";
import { Link } from "react-router";

import type { ChecklistView } from "@/plan/api/schema";
import { Untrusted } from "@/plan/chat/visible-controls";
import { SentenceText } from "@/plan/concept/sentence-text";
import { STEP } from "@/plan/feature/steps";
import { attentionOf, byAttention } from "@/plan/home/attention";
import { FeatureMark } from "@/plan/home/feature-mark";
import { FeatureStatus } from "@/plan/home/feature-status";
import { ProgressStrip } from "@/plan/home/progress-strip";
import { featuresAtom } from "@/plan/state/atoms";
import { plural } from "@/plan/ui/plural";

import "./feature-card.css";

/** One folder, as DESIGN.md allows: the product directory when the feature has one, else its plans. */
function pathLine({ feature, has_product }: ChecklistView): string {
  return has_product ? `product/${feature}/` : `plans/${feature}/`;
}

/** The step the card marks as current: the server's, or Run once everything before it is done. */
function currentStep(checklist: ChecklistView): number | null {
  const { current } = checklist;
  if (current.kind === "step") return current.number;
  return current.kind === "ready" ? STEP.run : null;
}

/** "4 of 6 steps done, 2 with gaps"; the gap clause only when a step has one. */
function stepSummary({ steps }: ChecklistView): string {
  const done = steps.filter((step) => step.mark === "done").length;
  const gaps = steps.filter((step) => step.mark === "gap").length;
  const summary = `${done} of ${plural(steps.length, "step")} done`;
  return gaps > 0 ? `${summary}, ${gaps} with gaps` : summary;
}

/** The document counts in fine print, and "all done" once every step is. */
function Facts({ checklist, allDone }: { checklist: ChecklistView; allDone: boolean }) {
  return (
    <div className="feature-card__facts plan-fine">
      <span>{plural(checklist.sections.length, "section")}</span>
      <span>{plural(checklist.plans.total, "plan")}</span>
      {allDone && (
        <span className="feature-card__done">
          <CircleCheckBig aria-hidden="true" className="tone-completed" />
          all done
        </span>
      )}
    </div>
  );
}

export function FeatureCard({ checklist }: { checklist: ChecklistView }) {
  const headingId = useId();
  const summaryId = useId();
  const statusId = useId();
  const attention = attentionOf(checklist);
  const { running, questions, findings, untested } = attention;
  // The status list renders only with something in it; the link is described by it only then.
  const status = running + questions + findings + untested > 0 ? ` ${statusId}` : "";

  return (
    <Link
      to={`/f/${encodeURIComponent(checklist.feature)}`}
      aria-labelledby={headingId}
      aria-describedby={`${summaryId}${status}`}
      data-quiet={attention.quiet || undefined}
      className="feature-card"
    >
      <div className="feature-card__top">
        <FeatureMark slug={checklist.feature} />
        <div className="feature-card__id">
          <h3 id={headingId} className="feature-card__title">
            <Untrusted text={checklist.feature} />
          </h3>
          <span className="feature-card__path">
            <Untrusted text={pathLine(checklist)} />
          </span>
        </div>
      </div>
      {checklist.pitch && (
        <p className="feature-pitch feature-card__pitch">
          <span style={{ unicodeBidi: "isolate", direction: "ltr" }}>
            <SentenceText text={checklist.pitch} />
          </span>
        </p>
      )}
      <div className="feature-card__foot">
        <Progress checklist={checklist} summaryId={summaryId} />
        <FeatureStatus id={statusId} attention={attention} />
        <Facts checklist={checklist} allDone={attention.allDone} />
      </div>
    </Link>
  );
}

/** The strip and the step count, read as one line; the count describes the card's link. */
function Progress({ checklist, summaryId }: { checklist: ChecklistView; summaryId: string }) {
  return (
    <div className="feature-card__meta">
      <ProgressStrip
        steps={checklist.steps}
        current={currentStep(checklist)}
        running={checklist.plans.running > 0}
      />
      <span id={summaryId} className="feature-card__count">
        {stepSummary(checklist)}
      </span>
    </div>
  );
}

/**
 * The list is the grid. `lead` renders as its first item, across the whole row, so the home
 * invitation heads the features instead of taking a card's place among them. Features come in
 * reading order (`byAttention`): running work first, finished work last.
 */
export function FeatureCards({ lead }: { lead?: ReactNode }) {
  const features = useAtomValue(featuresAtom);
  const ordered = useMemo(() => byAttention(features), [features]);
  if (ordered.length === 0) return null;

  return (
    <ul aria-label="Features" className="card-grid">
      {lead && <li className="card-grid__lead">{lead}</li>}
      {ordered.map((checklist) => (
        <li key={checklist.feature} className="min-w-0">
          <FeatureCard checklist={checklist} />
        </li>
      ))}
    </ul>
  );
}
