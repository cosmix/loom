import { Badge, BadgeAlert, BadgeCheck, type LucideIcon } from "lucide-react";
import type { JSX } from "react";

import type { SettleState } from "@/plan/api/schema-trace";

import { isMoved } from "./model";

export type SealKind = "settled" | "draft" | "attention";

/** The one glyph map of the settle states; the spine rows and the coverage marks draw from it. */
export const SEAL_ICONS: Record<SealKind, LucideIcon> = {
  settled: BadgeCheck,
  draft: Badge,
  attention: BadgeAlert,
};

export function sealKind(state: SettleState): SealKind {
  if (state === "settled") return "settled";
  return isMoved(state) ? "attention" : "draft";
}

/**
 * A settle state as one glyph: a check seal (settled), an empty seal (draft) or an amber seal with
 * an exclamation (changed since settling). Decorative: the caller draws the state's words beside
 * it. `data-glyph` names the shape and `data-settle` the state, for the stylesheet's tone.
 */
export function SettleGlyph({ state }: { state: SettleState }): JSX.Element {
  const kind = sealKind(state);
  const Icon = SEAL_ICONS[kind];
  return (
    <span className="trace-glyph" data-glyph={kind} data-settle={state} aria-hidden="true">
      <Icon />
    </span>
  );
}
