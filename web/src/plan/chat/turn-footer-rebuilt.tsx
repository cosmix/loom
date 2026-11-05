import { Link } from "react-router";

import { pageHref, pageKeyOfSummaryId } from "@/plan/api/page-key";
import type { Turn } from "@/plan/api/schema-chat";
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
  const count = turn.summaries_rebuilding.length;
  if (!count) return null;
  return (
    <div className="space-y-1">
      {turn.open && (
        <p className="chat-rebuild-chip">
          {count} {count === 1 ? "summary" : "summaries"} rebuilding
        </p>
      )}
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
              {turn.open ? "Rebuilding" : "left for the background rebuild"}
            </li>
          );
        })}
      </ul>
    </div>
  );
}
