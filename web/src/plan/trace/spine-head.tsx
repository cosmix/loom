import type { LucideIcon } from "lucide-react";
import { Link } from "react-router";

import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { GLYPH_ICONS } from "@/plan/feature/spine-row";
import { INSTRUMENT_GLYPHS } from "@/plan/ui/glyphs";

import { frontierHead, type HeadGlyph } from "./model";
import { useTrace } from "./state";

const HEAD_ICONS: Record<HeadGlyph, LucideIcon> = {
  findings: INSTRUMENT_GLYPHS.findings,
  "not-written": GLYPH_ICONS["not-written"],
  attention: GLYPH_ICONS.attention,
  draft: GLYPH_ICONS.draft,
};

/**
 * The head of the spine: the trace's top attention item as one compact "Next" link, a well like the
 * instruments below it. The link holds the label, the glyph with the verb and name on one line
 * (cut by an ellipsis, in full in its tooltip), the reason, and a muted further line when a plan
 * is flagged with it. Nothing when the frontier is empty or the trace is not loaded.
 */
export function SpineHead({ feature }: { feature: string }) {
  const { trace } = useTrace(feature);
  const head = trace ? frontierHead(trace, feature) : null;
  if (!head) return null;
  const Icon = HEAD_ICONS[head.glyph];
  return (
    <Link to={head.to} className="plan-spine-next">
      <span className="plan-spine-next__label" aria-hidden="true">
        Next
      </span>
      <span className="plan-spine-next__what" title={visibleControls(`${head.verb} ${head.name}`)}>
        <span className="plan-spine-next__glyph" data-glyph={head.glyph} aria-hidden="true">
          <Icon />
        </span>
        <span className="plan-spine-next__verb">{head.verb}</span>{" "}
        <span className="plan-spine-next__name" data-slug={head.nameIsSlug ? "" : undefined}>
          <Untrusted text={head.name} />
        </span>
      </span>{" "}
      <span className="plan-spine-next__reason">{head.reason}</span>
      {head.detail !== null && (
        <>
          {" "}
          <span className="plan-spine-next__reason">
            <Untrusted text={head.detail} />
          </span>
        </>
      )}
    </Link>
  );
}
