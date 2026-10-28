import { Link } from "react-router";

import { BusyRoundel } from "@/aurora-ui/feedback/BusyRoundel";
import type { Turn } from "@/plan/api/schema-chat";
import { RebuiltSummaryExtras } from "./rebuilt-summary-extras";
import { Untrusted } from "./visible-controls";

/**
 * In-app address of a summary id from the server. Leading slashes and empty or dot segments are
 * dropped so the id can never form a protocol-relative (`//host`) or parent-relative link.
 */
export function summaryLink(id: string): string {
  const segments = id.split("/").filter((part) => part !== "" && part !== "." && part !== "..");
  const page = segments[0] === "plans" ? ["engineering", ...segments.slice(1)] : segments;
  return `/${page.map(encodeURIComponent).join("/")}`;
}

export function RebuiltSummaries({ turn }: { turn: Turn }) {
  if (!turn.summaries_rebuilding.length) return null;
  return (
    <div>
      <p>Rebuilt summaries</p>
      <ul>
        {turn.summaries_rebuilding.map((id) => (
          <li key={id} className="flex flex-wrap items-center gap-2">
            <Link className="underline" to={summaryLink(id)}>
              <Untrusted text={id} />
            </Link>
            {turn.open ? (
              <>
                <span aria-hidden="true">
                  <BusyRoundel busy size={12} busyLabel="" />
                </span>{" "}
                Rebuilding
              </>
            ) : (
              "left for the background rebuild"
            )}
            <RebuiltSummaryExtras id={id} turn={turn} />
          </li>
        ))}
      </ul>
    </div>
  );
}
