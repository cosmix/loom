import { useAtomValue } from "jotai/react";
import { Clock, Flag, Layers } from "lucide-react";
import { useMemo } from "react";

import { pageHref, summaryKey } from "@/plan/api/page-key";
import type { ConceptPage } from "@/plan/api/schema-concept";
import type { DocView } from "@/plan/api/schema-files";
import { planStateText, summaryStatus } from "@/plan/concept/summary-text";
import { sourceHref } from "@/plan/feature/source-href";
import { repoRel, toAddress } from "@/plan/files/address";
import { openQuestionsAtom } from "@/plan/review/live";
import { PressureTestAction, PressureTestDetail } from "@/plan/run/plan-run-actions";
import { featuresAtom } from "@/plan/state/atoms";
import { settlePoints } from "@/plan/trace/doc-model";
import type { HeadingChip, HeadingChipLink, HeadingCrumb } from "@/plan/ui/doc-heading";
import {
  OTHER_GLYPHS,
  PLAN_STATE_WORD_GLYPHS,
  SUMMARY_STATE_GLYPHS,
  type PlanState,
} from "@/plan/ui/glyphs";
import { plural } from "@/plan/ui/plural";

import { pressureStatus } from "./pressure-words";

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

/** Running is `go` with the rotating arrows (the chip is a state word), Done is `ok`; Written and Retired stay neutral. */
export function planStateChip(state: PlanState): HeadingChip {
  const StateGlyph = PLAN_STATE_WORD_GLYPHS[state];
  return {
    key: "state",
    label: STATE_LABELS[state],
    icon: <StateGlyph />,
    tone: state === "running" ? "accent" : state === "done" ? "ok" : "neutral",
  };
}

export { NOT_PRESSURE_TESTED, PRESSURE_TESTED, pressureIcon, pressureWhen } from "./pressure-words";

/**
 * A plan's pressure record as a heading status. A written plan passes its `path`: the status is
 * then the Pressure-test button itself, whose label is the state (`PressureTestAction`), with the
 * Codex configuration notice as the row's detail. Without a path it is the state's glyph and
 * words alone, the record's date as its tooltip.
 */
export function pressureChip(
  date: string | null,
  active: boolean,
  path: string | null = null,
): HeadingChip {
  const { label, Icon, tone } = pressureStatus(date, active);
  const chip: HeadingChip = { key: "pressure", label, icon: <Icon />, tone };
  if (date !== null && !active) chip.title = date;
  if (path === null) return chip;
  return {
    ...chip,
    control: <PressureTestAction path={path} date={date} active={active} />,
    detail: <PressureTestDetail />,
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

/**
 * "<n> My calls" (warn, the flag the settle confirmation draws for them) linking to the first one:
 * the other kind of point settling asks about, beside the open questions. `doc` is the document's
 * text as the page holds it; nothing while it is not loaded or holds none. `inFeature` links as
 * `useOpenQuestionChips` does.
 */
export function useMyCallChips(
  path: string | null,
  doc: Pick<DocView, "text" | "sections"> | null,
  inFeature = false,
): HeadingChip[] {
  const calls = useMemo(
    () => (doc === null ? [] : settlePoints(doc).filter((point) => point.kind === "call")),
    [doc],
  );
  if (path === null || calls.length === 0) return [];
  const anchor = calls[0]?.where?.anchor;
  return [
    {
      key: "my-calls",
      label: plural(calls.length, "My call"),
      icon: <Flag />,
      tone: "warning",
      to: inFeature ? sourceHref(path, { anchor }) : toAddress(path, anchor),
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

/** A plan's state as the mark of its chip: the state word's glyph and tone, then the word. */
function planLinkState(state: string): HeadingChipLink["state"] {
  if (!Object.hasOwn(STATE_LABELS, state)) return { label: planStateText(state) };
  const known = state as PlanState;
  const StateGlyph = PLAN_STATE_WORD_GLYPHS[known];
  return {
    label: STATE_LABELS[known],
    icon: <StateGlyph />,
    tone: known === "running" ? "accent" : known === "done" ? "ok" : "neutral",
  };
}

/**
 * "Built by": the plans that implement a summary's document, each a chip of its two-digit number,
 * its title and its state, linked to its page. A plan whose slug carries no number goes by its
 * title alone.
 */
export function builtByChips(page: ConceptPage): HeadingChip[] {
  if (page.implemented_by.length === 0) return [];
  return [
    {
      key: "built-by",
      label: "Built by",
      links: page.implemented_by.map((plan) => {
        const number = slugNumber(plan.slug);
        return {
          ...(number === null ? { label: plan.title } : { label: number, name: plan.title }),
          state: planLinkState(plan.state),
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
