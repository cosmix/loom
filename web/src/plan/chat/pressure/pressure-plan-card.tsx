import { cn } from "cn";
import { useAtomValue } from "jotai";
import { ArrowLeftRight, Clock, FlaskConical, Gauge } from "lucide-react";
import { useEffect, useId, useState, type JSX, type ReactNode } from "react";

import { useNow } from "@/components/hooks/use-now";
import type { DocView } from "@/plan/api/schema-files";
import { fetchDoc } from "@/plan/files/api";
import { repoRel } from "@/plan/files/address";
import { planStateChip, relativeTime } from "@/plan/files/heading-parts";
import { standaloneAtom } from "@/plan/state/atoms";
import { CHIP_FILL, CHIP_ICON, type HeadingTone } from "@/plan/ui/doc-heading";
import { DocMark } from "@/plan/ui/doc-mark";
import type { PlanState } from "@/plan/ui/glyphs";

import { Untrusted, visibleControls } from "../visible-controls";

interface PlanFacts {
  /** The plan's real title, or null while neither source has answered. Never the slug. */
  title: string | null;
  state: PlanState | null;
  changed: string | null;
  /** `undefined` until the document loads, `null` when the plan was never tested. */
  tested: string | null | undefined;
}

// The standalone list arrives with the project and carries the title already; the document view
// adds the state, the last change and the pressure date, and is the fallback for a plan the list
// does not hold (a plan under a feature).
function usePlanFacts(plan: string): PlanFacts {
  const listed = useAtomValue(standaloneAtom).find(
    (entry) => repoRel(entry.path) === repoRel(plan),
  );
  const [loaded, setLoaded] = useState<{ plan: string; view: DocView } | null>(null);
  useEffect(() => {
    let current = true;
    void fetchDoc(plan).then((result) => {
      if (current && result.ok) setLoaded({ plan, view: result.value });
    });
    return () => {
      current = false;
    };
  }, [plan]);
  const view = loaded?.plan === plan ? loaded.view : null;
  return {
    title: listed?.title ?? view?.title ?? null,
    state: view?.plan?.state ?? listed?.state ?? null,
    changed: view?.last_change ?? null,
    tested: view?.plan ? view.plan.pressure_date : undefined,
  };
}

function StatusChip({
  icon,
  tone = "neutral",
  title,
  children,
}: {
  icon: ReactNode;
  tone?: HeadingTone;
  title?: string;
  children: ReactNode;
}): JSX.Element {
  return (
    <span
      title={title === undefined ? undefined : visibleControls(title)}
      className={cn(
        "inline-flex h-6 items-center gap-1.5 rounded-[6px] px-2 text-xs font-medium",
        CHIP_FILL[tone],
      )}
    >
      <span aria-hidden="true" className={cn("inline-flex [&_svg]:size-3.5", CHIP_ICON[tone])}>
        {icon}
      </span>
      {children}
    </span>
  );
}

function PlanChips({ facts }: { facts: PlanFacts }): JSX.Element {
  const now = useNow(60_000);
  const state = facts.state === null ? null : planStateChip(facts.state);
  return (
    <div className="flex flex-wrap gap-1.5 pt-1">
      {state && (
        <StatusChip icon={state.icon} tone={state.tone}>
          {state.label}
        </StatusChip>
      )}
      {facts.changed !== null && (
        <StatusChip icon={<Clock />} title={facts.changed}>
          Changed {relativeTime(facts.changed, now)}
        </StatusChip>
      )}
      {facts.tested === null && (
        <StatusChip icon={<Gauge />} tone="warning">
          Never tested
        </StatusChip>
      )}
      {typeof facts.tested === "string" && (
        <StatusChip icon={<FlaskConical />} tone="ok">
          Tested <Untrusted text={facts.tested} />
        </StatusChip>
      )}
    </div>
  );
}

export function PressurePlanCard({
  plan,
  plans,
  setPlan,
}: {
  plan: string;
  plans: readonly string[];
  setPlan: (plan: string) => void;
}): JSX.Element {
  const facts = usePlanFacts(plan);
  const fileName = plan.split("/").at(-1) ?? plan;
  return (
    <div className="grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 rounded-[12px] border border-hairline bg-background px-3.5 py-3">
      <DocMark kind="plan" size="lg" />
      <div className="min-w-0 [overflow-wrap:anywhere]">
        {facts.title !== null && (
          <h3 className="text-[14.5px] leading-snug font-semibold">
            <Untrusted text={facts.title} />
          </h3>
        )}
        <p className="font-mono text-xs text-muted-foreground">
          <Untrusted text={fileName} />
        </p>
        <PlanChips facts={facts} />
      </div>
      <PlanChoice plan={plan} plans={plans} setPlan={setPlan} />
    </div>
  );
}

function PlanChoice({
  plan,
  plans,
  setPlan,
}: {
  plan: string;
  plans: readonly string[];
  setPlan: (plan: string) => void;
}): JSX.Element {
  const id = useId();
  return (
    <div className="relative inline-flex h-7 shrink-0 items-center gap-1.5 rounded-[7px] border border-border px-2 text-[12.5px] font-medium hover:bg-muted focus-within:outline-2 focus-within:outline-ring">
      {/* The accessible name contains the visible word "Change" (WCAG 2.5.3). */}
      <label htmlFor={id} className="sr-only">
        Change plan
      </label>
      <ArrowLeftRight aria-hidden="true" className="size-3.5" />
      <span aria-hidden="true">Change</span>
      <select
        id={id}
        value={plan}
        required
        className="absolute inset-0 size-full cursor-pointer opacity-0"
        onChange={(event) => setPlan(event.currentTarget.value)}
      >
        {plans.map((option) => (
          <option key={option} value={option}>
            {visibleControls(option)}
          </option>
        ))}
      </select>
    </div>
  );
}
