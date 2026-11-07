import { Check, CircleDashed, CircleDot } from "lucide-react";
import type { LucideIcon } from "lucide-react";

import type { SettleState } from "@/plan/api/schema-trace";

import { STATE_WORDS, type RowState } from "./coverage-model";
import { settleWords } from "./doc-model";
import { SEAL_ICONS, SettleGlyph } from "./settle-glyph";

import "./trace.css";

// The glyph-and-word marks of the coverage page. The glyph is decoration: the word beside it
// carries the meaning, and `data-mark` names the kind for the stylesheet.

const ROW_ICONS: Record<RowState, LucideIcon> = {
  built: Check,
  planned: CircleDot,
  changed: SEAL_ICONS.attention,
  uncovered: CircleDashed,
};

/** A heading row's state: Built, Planned, Changed since covered or Not covered. */
export function StateMark({ state }: { state: RowState }) {
  const Icon = ROW_ICONS[state];
  return (
    <span className="coverage-mark" data-mark={state}>
      <Icon aria-hidden="true" />
      <span>{STATE_WORDS[state]}</span>
    </span>
  );
}

/** A section's settle state, for a document the reader is asked to settle. */
export function SettleMark({ state }: { state: SettleState }) {
  return (
    <span
      className="coverage-mark"
      data-mark={state === "settled" || state === "draft" ? state : "changed"}
    >
      <SettleGlyph state={state} />
      <span>{settleWords(state, "section").chip}</span>
    </span>
  );
}
