import { BookOpen, CircleCheckBig, Gauge, ListChecks } from "lucide-react";
import { useAtomValue } from "jotai/react";
import { useId, type ReactNode } from "react";
import { Link } from "react-router";

import type { ChecklistView } from "@/plan/api/schema";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { SentenceText } from "@/plan/concept/sentence-text";
import { plural } from "@/plan/feature/steps";
import { FeatureMark } from "@/plan/home/feature-mark";
import { ProgressStrip } from "@/plan/home/progress-strip";
import { featuresAtom } from "@/plan/state/atoms";
import { ActivityGlyph } from "@/plan/ui/activity-glyphs";

import "./feature-card.css";

const METAPLAN_STEP = 3;
const RUN_STEP = 7;

/** The two folders a feature is a pair of, or which one is missing. */
function pathLine(checklist: ChecklistView): string {
  const { feature, has_product, plans, steps } = checklist;
  const metaplanDone = steps.some((s) => s.number === METAPLAN_STEP && s.mark === "done");
  if (!has_product) return `plans/${feature}/ · no product spec`;
  if (plans.total === 0 && !metaplanDone) return `product/${feature}/ · no plans yet`;
  return `product/${feature}/ · plans/${feature}/`;
}

/** The step the card marks as current: the server's, or Run once everything before it is done. */
function currentStep(checklist: ChecklistView): number | null {
  const { current } = checklist;
  if (current.kind === "step") return current.number;
  return current.kind === "ready" ? RUN_STEP : null;
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
            <Untrusted text={checklist.title} />
          </h3>
          <span className="feature-card__path">
            <Untrusted text={pathLine(checklist)} />
          </span>
        </div>
      </div>
      {checklist.pitch && (
        <p className="feature-pitch feature-card__pitch">
          <span style={{ unicodeBidi: "isolate", direction: "ltr" }}>
            <SentenceText text={visibleControls(checklist.pitch)} />
          </span>
        </p>
      )}
      <ProgressStrip
        steps={checklist.steps}
        current={currentStep(checklist)}
        running={checklist.plans.running > 0}
      />
      <Facts checklist={checklist} />
      <span id={summaryId} className="sr-only">
        {`${done} of ${plural(checklist.steps.length, "step")} done, ${gaps} with gaps`}
      </span>
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
