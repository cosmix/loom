import { useAtomValue } from "jotai/react";
import { CircleAlert } from "lucide-react";
import type { JSX, ReactNode } from "react";

import type { TreeEntry } from "@/plan/api/schema-files";
import { visibleControls } from "@/plan/chat/visible-controls";
import { validationAtom } from "@/plan/files/atoms";
import { NOT_PRESSURE_TESTED, PRESSURE_TESTED, pressureIcon } from "@/plan/files/heading-parts";
import { TreeEntryExtras } from "@/plan/files/tree-entry-extras";
import { PLAN_STATE_GLYPHS, type Glyph } from "@/plan/ui/glyphs";

/**
 * A glyph standing for a word: the word is read from `.sr-only` and shown as the glyph's
 * tooltip; the glyph itself is hidden from assistive technology so the word is announced once.
 */
function Quiet({ icon: Icon, word, tone }: { icon: Glyph; word: string; tone?: string }) {
  return (
    <span className="plan-tree__signal">
      <span className="plan-tree__glyph" data-tone={tone} title={word} aria-hidden="true">
        <Icon />
      </span>
      <span className="sr-only">{word}</span>
    </span>
  );
}

/** Something that needs the reader: its glyph and its words, both shown. */
function Loud({ icon: Icon, tone, children }: { icon: Glyph; tone: string; children: ReactNode }) {
  return (
    <span className="plan-tree__signal" data-loud="">
      <span className="plan-tree__glyph" data-tone={tone} aria-hidden="true">
        <Icon />
      </span>
      {children}
    </span>
  );
}

/**
 * A plan's state. Only a running plan says so aloud; a written plan shows its pressure record
 * instead, a retired one its glyph, and a done one nothing but its spoken word: finished work is
 * folded away and needs no mark of its own.
 */
function PlanState({ entry }: { entry: TreeEntry }): JSX.Element | null {
  const { state } = entry;
  if (entry.kind !== "plan" || state === null) return null;
  if (state === "running") {
    return (
      <Loud icon={PLAN_STATE_GLYPHS.running} tone="running">
        running
      </Loud>
    );
  }
  if (state === "retired") return <Quiet icon={PLAN_STATE_GLYPHS.retired} word="retired" />;
  if (state === "done") return <span className="sr-only">done</span>;
  const date = entry.pressure_date;
  const record =
    date === null ? NOT_PRESSURE_TESTED : `${PRESSURE_TESTED} ${visibleControls(date)}`;
  return (
    <>
      <span className="sr-only">written</span>
      <Quiet icon={pressureIcon(date)} word={record} tone={date === null ? undefined : "done"} />
    </>
  );
}

/** A plan's validation error count, live from the editor once it has checked the plan. */
export function usePlanErrors(entry: TreeEntry): number {
  const validation = useAtomValue(validationAtom);
  if (entry.kind !== "plan" || entry.state === null) return 0;
  const live = entry.path ? validation[entry.path] : undefined;
  return live ? live.errors.length : entry.errors;
}

function Errors({ entry }: { entry: TreeEntry }): JSX.Element | null {
  const errors = usePlanErrors(entry);
  if (errors === 0) return null;
  return (
    <Loud icon={CircleAlert} tone="blocked">
      {errors === 1 ? "1 error" : `${errors} errors`}
    </Loud>
  );
}

/**
 * The row's end: what needs attention in words (errors, a running plan, a missing document), the
 * rest as quiet glyphs, then the review marks (open questions, changed since you looked).
 */
export function EntrySignals({ entry }: { entry: TreeEntry }): JSX.Element {
  return (
    <span className="plan-tree__signals">
      {entry.gap && <span className="plan-tree__word">missing</span>}
      {entry.locked && <span className="sr-only">Locked</span>}
      <Errors entry={entry} />
      <PlanState entry={entry} />
      <TreeEntryExtras entry={entry} />
    </span>
  );
}
