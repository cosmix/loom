import { CircleDashed } from "lucide-react";
import { Link } from "react-router";

import { Untrusted } from "@/plan/chat/visible-controls";
import { featureHref } from "@/plan/feature/use-feature";
import { PLAN_STATE_WORD_GLYPHS, type PlanState } from "@/plan/ui/glyphs";

/**
 * A plan row's state as a glyph and a word (DESIGN.md, Status). The server's `state` is a free
 * string on the checklist, so an unknown one gets a neutral glyph; only a done plan takes a tone.
 */
export function PlanStateStatus({ state }: { state: string }) {
  // Own keys only: a state named "constructor" must not pick a function off the prototype.
  const Icon = Object.hasOwn(PLAN_STATE_WORD_GLYPHS, state)
    ? PLAN_STATE_WORD_GLYPHS[state as PlanState]
    : CircleDashed;
  return (
    <span className="plan-status">
      <Icon className={state === "done" ? "tone-completed" : undefined} aria-hidden="true" />
      <Untrusted text={state} />
    </span>
  );
}

/** A plan's slug as a link to its concept page, truncated in a narrow row. */
export function PlanName({ feature, slug }: { feature: string; slug: string }) {
  return (
    <span className="plan-page__name">
      <Link to={featureHref(feature, `engineering/${encodeURIComponent(slug)}`)}>
        <Untrusted text={slug} />
      </Link>
    </span>
  );
}
