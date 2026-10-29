import { Link } from "react-router";

import { pageHref, pageKeyOfSummaryId } from "@/plan/api/page-key";
import type { Turn } from "@/plan/api/schema-chat";
import { ActivityGlyph } from "@/plan/ui/activity-glyphs";
import { RebuiltSummaryExtras } from "./rebuilt-summary-extras";
import { usePlanFeature } from "./turn-footer-plans";
import { Untrusted } from "./visible-controls";

/** In-app address of a validated server summary id, with an optional sentence fragment. */
export function summaryLink(
  id: string,
  planFeature: (slug: string) => string | null,
  anchor: string | null = null,
): string | null {
  const key = pageKeyOfSummaryId(id, planFeature);
  return key === null ? null : `${pageHref(key)}${anchor ? `#${encodeURIComponent(anchor)}` : ""}`;
}

export function RebuiltSummaries({ turn }: { turn: Turn }) {
  const planFeature = usePlanFeature();
  if (!turn.summaries_rebuilding.length) return null;
  return (
    <div>
      <p>Rebuilt summaries</p>
      <ul>
        {turn.summaries_rebuilding.map((id) => {
          const href = summaryLink(id, planFeature);
          const title = <Untrusted text={id} />;
          return (
            <li key={id} className="flex flex-wrap items-center gap-2">
              {href === null ? (
                title
              ) : (
                <Link className="underline" to={href}>
                  {title}
                </Link>
              )}
              {turn.open ? (
                <>
                  <ActivityGlyph kind="summaries" className="size-3" /> Rebuilding
                </>
              ) : (
                "left for the background rebuild"
              )}
              <RebuiltSummaryExtras id={id} turn={turn} />
            </li>
          );
        })}
      </ul>
    </div>
  );
}
