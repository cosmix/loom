import { BookOpen, CircleCheckBig, Gauge, ListChecks } from "lucide-react";
import { useAtomValue } from "jotai/react";
import { useId, type ReactNode } from "react";
import { Link } from "react-router";

import type { ChecklistView } from "@/plan/api/schema";
import { Untrusted } from "@/plan/chat/visible-controls";
import { SentenceText } from "@/plan/concept/sentence-text";
import { plural } from "@/plan/ui/plural";
import { FeatureMark } from "@/plan/home/feature-mark";
import { STEP } from "@/plan/feature/steps";
import { ProgressStrip } from "@/plan/home/progress-strip";
import { featuresAtom } from "@/plan/state/atoms";
import { ActivityGlyph } from "@/plan/ui/activity-glyphs";

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

function Fact({ icon, children }: { icon: ReactNode; children: string }) {
  return (
    <span className="feature-card__fact">
      {icon}
      {children}
    </span>
  );
}

function Facts({ checklist }: { checklist: ChecklistView }) {
  const { plans, sections, untested, steps } = checklist;
  const allDone = steps.length > 0 && steps.every((step) => step.mark === "done");

  return (
    <div className="feature-card__facts">
      {plans.running > 0 && (
        <Fact icon={<ActivityGlyph kind="running" className="size-3.5" />}>
          {`${plans.running} running`}
        </Fact>
      )}
      <Fact icon={<BookOpen aria-hidden="true" />}>{plural(sections.length, "section")}</Fact>
      <Fact icon={<ListChecks aria-hidden="true" />}>{plural(plans.total, "plan")}</Fact>
      {untested > 0 && <Fact icon={<Gauge aria-hidden="true" />}>{`${untested} untested`}</Fact>}
      {allDone && <Fact icon={<CircleCheckBig aria-hidden="true" />}>all done</Fact>}
    </div>
  );
}

export function FeatureCard({ checklist }: { checklist: ChecklistView }) {
  const headingId = useId();
  const summaryId = useId();
  const done = checklist.steps.filter((step) => step.mark === "done").length;
  const gaps = checklist.steps.filter((step) => step.mark === "gap").length;

  return (
    <Link
      to={`/f/${encodeURIComponent(checklist.feature)}`}
      aria-labelledby={headingId}
      aria-describedby={summaryId}
      className="feature-card"
    >
      <div className="feature-card__top">
        <FeatureMark slug={checklist.feature} />
        <div className="feature-card__id">
          <h3 id={headingId} className="feature-card__title">
            <Untrusted text={checklist.feature} />
          </h3>
          <span className="feature-card__path plan-fine">
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
      <ProgressStrip
        steps={checklist.steps}
        current={currentStep(checklist)}
        running={checklist.plans.running > 0}
      />
      <div className="feature-card__meta">
        <span id={summaryId} className="plan-fine">
          {`${done} of ${plural(checklist.steps.length, "step")} done, ${gaps} with gaps`}
        </span>
        <Facts checklist={checklist} />
      </div>
    </Link>
  );
}

/**
 * The list is the grid. `lead` renders as its first item, so a card placed there (the home
 * invitation) flows through the same tracks as the features instead of owning a row.
 */
export function FeatureCards({ lead }: { lead?: ReactNode }) {
  const features = useAtomValue(featuresAtom);
  if (features.length === 0) return null;

  return (
    <ul aria-label="Features" className="card-grid">
      {lead && <li className="card-grid__lead">{lead}</li>}
      {features.map((checklist) => (
        <li key={checklist.feature} className="min-w-0">
          <FeatureCard checklist={checklist} />
        </li>
      ))}
    </ul>
  );
}
