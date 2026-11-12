import { Check } from "lucide-react";
import { useId, type JSX } from "react";
import { Link } from "react-router";

import type { SessionView } from "@/plan/api/schema-chat";
import { CHANGES_PATH } from "@/plan/review/changed-since";
import { Spinner } from "@/plan/ui/spinner";
import type { ClosingTick, LeftBehind } from "./closing";
import { StartingBrief } from "./start-chat";
import { ChangeCounts } from "./tool-row";
import { Untrusted } from "./visible-controls";

/**
 * The ending, while the closing write runs: the pane's one spinner, and a tick per document the
 * write has reached. The closing prompt itself is never drawn (transcript.tsx). It sits inside the
 * polite `role="log"` transcript, so it is not a live region of its own: tick changes would be
 * announced twice.
 */
export function WrapUpCard({ ticks }: { ticks: ClosingTick[] }): JSX.Element {
  return (
    <div className="chat-wrap" aria-live="off">
      <div className="chat-wrap-head">
        <span className="chat-wrap-mark">
          <Spinner />
        </span>
        <div>
          <h3>Writing down what we decided</h3>
          <p>Putting this chat&apos;s decisions and open questions into the documents.</p>
        </div>
      </div>
      {ticks.length > 0 && (
        <ul className="chat-wrap-ticks">
          {ticks.map((tick) => (
            <li key={tick.path} data-state={tick.state}>
              <span className="chat-wrap-tick" aria-hidden="true">
                {tick.state === "done" && <Check size={12} />}
              </span>
              <span className="sr-only">{tick.state === "done" ? "Written: " : "Writing: "}</span>
              <span className="chat-wrap-path">
                <Untrusted text={tick.path} />
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** "documents", "decision": the noun after the count, which the pill sets apart. */
function noun(count: number, singular: string): string {
  return count === 1 ? singular : `${singular}s`;
}

function Counts({ summary }: { summary: LeftBehind }): JSX.Element | null {
  const { documents, decisions, questions } = summary;
  const counts = [
    { glyph: "✓", words: `${noun(documents.length, "document")} changed`, count: documents.length },
    { glyph: "✓", words: noun(decisions.length, "decision"), count: decisions.length },
    { glyph: "?", words: noun(questions.length, "open question"), count: questions.length },
  ].filter((entry) => entry.count > 0);
  if (counts.length === 0) return null;
  return (
    <ul className="chat-left-behind-counts">
      {counts.map((entry) => (
        <li key={entry.words} data-glyph={entry.glyph === "?" ? "question" : "check"}>
          <span aria-hidden="true">{entry.glyph}</span>{" "}
          <span className="chat-left-behind-n">{entry.count}</span> {entry.words}
        </li>
      ))}
    </ul>
  );
}

function Documents({ documents }: { documents: LeftBehind["documents"] }): JSX.Element | null {
  if (documents.length === 0) return null;
  return (
    <ul className="chat-left-behind-docs">
      {documents.map((entry) => (
        <li key={entry.path}>
          <i aria-hidden="true" />
          <span className="chat-left-behind-path">
            <Untrusted text={entry.path} />
          </span>
          {(entry.added !== null || entry.removed !== null) && (
            <>
              {" "}
              <ChangeCounts added={entry.added} removed={entry.removed} />
            </>
          )}
        </li>
      ))}
    </ul>
  );
}

/** What an ended chat left behind: documents it changed, what it settled, what it left open. */
export function LeftBehindCard({
  summary,
  plannerFinished = false,
}: {
  summary: LeftBehind;
  plannerFinished?: boolean;
}): JSX.Element {
  const headingId = useId();
  const { documents, decisions, questions } = summary;
  const empty = documents.length + decisions.length + questions.length === 0;
  return (
    <section className="chat-left-behind" aria-labelledby={headingId}>
      <h3 id={headingId}>
        {plannerFinished ? (
          "The planner finished this chat"
        ) : (
          <>Here&apos;s what this chat left behind</>
        )}
      </h3>
      <Counts summary={summary} />
      <Documents documents={documents} />
      {decisions.length > 0 && (
        <>
          <h4>Decided</h4>
          <ol>
            {decisions.map((text, index) => (
              <li key={index}>
                <Untrusted text={text} />
              </li>
            ))}
          </ol>
        </>
      )}
      {questions.length > 0 && (
        <>
          <h4>Still open</h4>
          <ul className="chat-left-behind-questions">
            {questions.map((text, index) => (
              <li key={index}>
                <Untrusted text={text} />
              </li>
            ))}
          </ul>
        </>
      )}
      {empty && <p className="chat-left-behind-empty">Nothing was decided or changed.</p>}
      {documents.length > 0 && (
        <Link to={CHANGES_PATH} className="chat-left-behind-link">
          See the changes
        </Link>
      )}
    </section>
  );
}

/** The starting state: the pane's one spinner, a title, and the session's brief line under it. */
export function StartingCard({ session }: { session: SessionView }): JSX.Element {
  return (
    <div className="chat-starting">
      <Spinner />
      <div className="chat-starting-text">
        <p className="chat-starting-title">Starting the session</p>
        <StartingBrief session={session} />
      </div>
    </div>
  );
}
