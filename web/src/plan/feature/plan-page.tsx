import type { ReactNode } from "react";
import { Link } from "react-router";

import type { ChecklistView } from "@/plan/api/schema";
import { Untrusted } from "@/plan/chat/visible-controls";
import { featureHref } from "@/plan/feature/use-feature";
import { plural } from "@/plan/ui/plural";

import "./pages.css";

/** The head of a feature page: the eyebrow, the page title and an optional sentence under it. */
export function PageHead({
  eyebrow,
  title,
  lede,
}: {
  eyebrow: string;
  title: string;
  lede?: ReactNode;
}) {
  return (
    <header className="plan-page__head">
      <p className="plan-eyebrow">
        <Untrusted text={eyebrow} />
      </p>
      <h1 className="plan-page-title">{title}</h1>
      {lede && <p className="plan-page__lede">{lede}</p>}
    </header>
  );
}

/**
 * The frame of the Run page: the feature slug and page label as the eyebrow,
 * the page title, and an optional sentence under it, then the page's content.
 */
export function PlanPage({
  feature,
  label,
  title,
  lede,
  children,
}: {
  feature: string;
  label: string;
  title: string;
  lede?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="plan-page">
      <PageHead eyebrow={`${feature} · ${label}`} title={title} lede={lede} />
      {children}
    </div>
  );
}

/** A feature with no plan: one sentence naming what comes first, and the way to Plans. */
export function NoPlans({ feature, sentence }: { feature: string; sentence: string }) {
  return (
    <div className="plan-page__empty">
      <p className="plan-empty">{sentence}</p>
      <Link to={featureHref(feature, "engineering")} className="plan-page__link">
        Go to Plans
      </Link>
    </div>
  );
}

/**
 * Plans the metaplans list that have no file yet. `plan_rows` holds only plans with a file, so
 * `plans.listed` (every listed plan) minus the listed rows is the count still to write.
 */
function unwrittenCount(checklist: ChecklistView): number {
  const written = checklist.plan_rows.filter((row) => row.listed).length;
  return Math.max(0, checklist.plans.listed - written);
}

/** Under a page's rows: how many listed plans are not written yet, and the way to write them. */
export function UnwrittenPlans({ checklist }: { checklist: ChecklistView }) {
  const count = unwrittenCount(checklist);
  if (count === 0) return null;
  const sentence = `${plural(count, "listed plan")} ${count === 1 ? "is" : "are"} not written yet.`;
  return (
    <div className="plan-page__section">
      <NoPlans feature={checklist.feature} sentence={sentence} />
    </div>
  );
}
