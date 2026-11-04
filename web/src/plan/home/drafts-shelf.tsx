import { useAtomValue } from "jotai";
import { useId } from "react";
import { Link } from "react-router";

import type { KickoffDraftView } from "@/plan/api/schema-kickoff";
import { Untrusted } from "@/plan/chat/visible-controls";
import { draftTitle, kickoffHref } from "@/plan/new/kickoff-state";
import { kickoffDraftsAtom } from "@/plan/state/atoms";

import "./drafts-shelf.css";

function newestFirst(a: KickoffDraftView, b: KickoffDraftView): number {
  return Date.parse(b.updated_at) - Date.parse(a.updated_at);
}

function firstLine(problem: string | null): string | null {
  const line = problem?.split("\n").find((text) => text.trim() !== "");
  return line?.trim() ?? null;
}

/**
 * Kickoffs that were started and not finished: a conversation still running, or one set aside.
 * Each row opens `/new?session=<id>`. Nothing renders when there are none.
 */
export function DraftsShelf() {
  const headingId = useId();
  const drafts = useAtomValue(kickoffDraftsAtom);
  if (drafts.length === 0) return null;
  return (
    <section aria-labelledby={headingId} className="drafts-shelf">
      <h2 id={headingId} className="plan-section-title">
        Unfinished
      </h2>
      <ul className="drafts-shelf__list">
        {[...drafts].sort(newestFirst).map((draft) => {
          const problem = firstLine(draft.problem);
          return (
            <li key={draft.session}>
              <Link to={kickoffHref(draft.session)} className="drafts-shelf__row">
                <span className="drafts-shelf__title">
                  <Untrusted text={draftTitle(draft)} />
                </span>
                {problem !== null && (
                  <span className="drafts-shelf__problem">
                    <Untrusted text={problem} />
                  </span>
                )}
                <span className="drafts-shelf__status plan-status" data-live={draft.live}>
                  <span aria-hidden="true" className="drafts-shelf__glyph">
                    {draft.live ? "●" : "○"}
                  </span>
                  <span className="drafts-shelf__state">{draft.live ? "open" : "set aside"}</span>
                </span>
              </Link>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
