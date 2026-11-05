import { useAtomValue } from "jotai/react";
import { Clock, Gauge, Layers } from "lucide-react";

import { pageHref, summaryKey } from "@/plan/api/page-key";
import type { ConceptPage } from "@/plan/api/schema-concept";
import { planStateText, summaryStatus } from "@/plan/concept/summary-text";
import { sourceHref } from "@/plan/feature/source-href";
import { repoRel, toAddress } from "@/plan/files/address";
import { openQuestionsAtom } from "@/plan/review/live";
import { featuresAtom } from "@/plan/state/atoms";
import { ActivityGlyph } from "@/plan/ui/activity-glyphs";
import type { HeadingChip, HeadingCrumb } from "@/plan/ui/doc-heading";
import {
  OTHER_GLYPHS,
  PLAN_STATE_GLYPHS,
  SUMMARY_STATE_GLYPHS,
  type PlanState,
} from "@/plan/ui/glyphs";
import { plural } from "@/plan/ui/plural";

const OpenQuestionsIcon = OTHER_GLYPHS.openQuestions;
const STATE_LABELS: Record<PlanState, string> = {
  written: "Written",
  running: "Running",
  done: "Done",
  retired: "Retired",
};

export function relativeTime(stamp: string, now: number): string {
  const seconds = Math.floor((now - Date.parse(stamp)) / 1000);
  if (!Number.isFinite(seconds)) return "unknown";
  const age = Math.max(0, seconds);
  if (age < 60) return `${age} s ago`;
  if (age < 3600) return `${Math.floor(age / 60)} min ago`;
  if (age < 86400) return `${Math.floor(age / 3600)} h ago`;
  return `${Math.floor(age / 86400)} d ago`;
}

/**
 * The feature's section count, null until the checklist arrives, when "Section 05 of 12" drops its
 * total.
 */
export function useFeatureFacts(feature: string | null): { sections: number | null } {
  const features = useAtomValue(featuresAtom);
  const checklist = features.find((candidate) => candidate.feature === feature);
  return { sections: checklist?.sections.length ?? null };
}

/** The first crumb of every feature document: the feature's slug, linking to its screen. */
export function featureCrumb(feature: string): HeadingCrumb {
  return {
    label: feature,
    mono: true,
    to: `/f/${encodeURIComponent(feature)}`,
    icon: <Layers />,
  };
}

/** "Section 05 of 12" from a `section_05` stem; without the total, "Section 05". */
export function sectionLabel(stem: string, total: number | null): string | null {
  const number = /^section_(\d+)$/.exec(stem)?.[1];
  if (number === undefined) return null;
  return total === null ? `Section ${number}` : `Section ${number} of ${total}`;
}

/** Running is `go` with the animated glyph, Done is `ok`; Written and Retired stay neutral. */
export function planStateChip(state: PlanState): HeadingChip {
  const StateGlyph = PLAN_STATE_GLYPHS[state];
  return {
    key: "state",
    label: STATE_LABELS[state],
    icon: state === "running" ? <ActivityGlyph kind="running" /> : <StateGlyph />,
    tone: state === "running" ? "accent" : state === "done" ? "ok" : "neutral",
  };
}

function today(): string {
  const now = new Date();
  return [
    now.getFullYear(),
    String(now.getMonth() + 1).padStart(2, "0"),
    String(now.getDate()).padStart(2, "0"),
  ].join("-");
}

/** The words and glyph of a plan's pressure record, shared by the page chip and the spine badge. */
export const PRESSURE_TESTED = "Pressure-tested";
export const NOT_PRESSURE_TESTED = "Not pressure-tested";
export const pressureIcon = (date: string | null) =>
  date === null ? Gauge : OTHER_GLYPHS.pressureTested;

/** "Not pressure-tested" (warn), "Pressure-tested <date>" (ok), or the live run (go). */
export function pressureChip(date: string | null, active: boolean): HeadingChip {
  const Icon = pressureIcon(date);
  if (active) {
    return { key: "pressure", label: "Being pressure-tested", icon: <Gauge />, tone: "accent" };
  }
  if (date === null) {
    return { key: "pressure", label: NOT_PRESSURE_TESTED, icon: <Icon />, tone: "warning" };
  }
  return {
    key: "pressure",
    label: `${PRESSURE_TESTED} ${date === today() ? "today" : date}`,
    icon: <Icon />,
    tone: "ok",
  };
}

export function changedChip(stamp: string, now: number): HeadingChip {
  return {
    key: "changed",
    label: `Changed ${relativeTime(stamp, now)}`,
    title: stamp,
    icon: <Clock />,
  };
}

/**
 * "<n> open questions" (warn) linking to the document's questions; nothing when it has none. A
 * summary page passes `inFeature` so the link stays inside the feature (spine included); file mode
 * links to its own address.
 */
export function useOpenQuestionChips(path: string | null, inFeature = false): HeadingChip[] {
  const counts = useAtomValue(openQuestionsAtom);
  const count = path === null ? undefined : counts[repoRel(path)];
  if (path === null || count === undefined || count === 0) return [];
  return [
    {
      key: "open-questions",
      label: plural(count, "open question"),
      icon: <OpenQuestionsIcon data-other-glyph="openQuestions" />,
      tone: "warning",
      to: inFeature
        ? sourceHref(path, { anchor: "open-questions" })
        : toAddress(path, "open-questions"),
    },
  ];
}

/** "§06": the section number of an `Implements:` link, zero-padded to two digits. */
export function sectionNumberLabel(number: number): string {
  return `§${String(number).padStart(2, "0")}`;
}

/** The two-digit number a plan's slug carries ("feat-planner-13" gives "13"), or null. */
export function slugNumber(slug: string): string | null {
  return /(?:^|[^0-9])(\d{2})(?![0-9])/.exec(slug)?.[1] ?? null;
}

/** The number a plan goes by in "Built by 05 · 09": its two-digit part, else its title. */
function planNumber(slug: string, title: string): string {
  return slugNumber(slug) ?? title;
}

/** "Built by 05 · 09": the plans that implement a summary's document, each linked to its page. */
export function builtByChips(page: ConceptPage): HeadingChip[] {
  if (page.implemented_by.length === 0) return [];
  return [
    {
      key: "built-by",
      label: "Built by",
      linksSeparator: " · ",
      links: page.implemented_by.map((plan) => {
        const label = planNumber(plan.slug, plan.title);
        const state = planStateText(plan.state);
        return {
          label,
          description: label === plan.title ? state : `${plan.title}, ${state}`,
          to: pageHref(summaryKey(page.feature, "engineering", plan.slug)),
        };
      }),
    },
  ];
}

function summaryLabel(state: NonNullable<ConceptPage["state"]>, status: string): string {
  if (state.out_of_date) return status;
  if (state.state === "current") return "Summary current";
  if (state.state === "rebuilding") return "Summary rebuilding";
  return status;
}

/** Whether the server is rebuilding this page's summary (its state, or the reason it is dashed). */
export function isRebuilding(page: ConceptPage): boolean {
  return page.state?.state === "rebuilding" || page.state?.out_of_date?.reason === "rebuilding";
}

export function summaryChips(page: ConceptPage, rebuilding: boolean): HeadingChip[] {
  const state = page.state;
  const status = summaryStatus(state);
  if (state === null || status === null) return [];
  const stale = state.out_of_date !== null || state.state === "stale";
  const current = state.state === "current" && state.out_of_date === null;
  const Glyph = SUMMARY_STATE_GLYPHS[rebuilding ? "rebuilding" : current ? "current" : "stale"];
  return [
    {
      key: "summary-status",
      label: summaryLabel(state, status),
      icon: <Glyph />,
      tone: stale ? "warning" : current ? "ok" : rebuilding ? "accent" : "neutral",
    },
  ];
}
