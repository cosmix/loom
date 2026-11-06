import type { JSX } from "react";
import { Link } from "react-router";

import type { TreeEntry } from "@/plan/api/schema-files";
import { visibleControls } from "@/plan/chat/visible-controls";
import { SentenceText } from "@/plan/concept/sentence-text";
import { toAddress } from "@/plan/files/address";
import { plainTitle } from "@/plan/files/tree-entry";
import { shortTitle } from "@/plan/files/tree-model";

/**
 * The one bar a section draws for its open questions: the first document holding some, how many
 * more do, and the total. It links where a row's question count links, the document's
 * `#open-questions`; the tooltip lists every document and its count.
 */
export function AttentionBar({
  docs,
}: {
  docs: { entry: TreeEntry; count: number }[];
}): JSX.Element | null {
  const first = docs[0];
  if (first === undefined || first.entry.path === null) return null;
  const total = docs.reduce((sum, doc) => sum + doc.count, 0);
  const words = `${total} open ${total === 1 ? "question" : "questions"}`;
  const tooltip = docs
    .map(({ entry, count }) => `${visibleControls(plainTitle(entry.title))} · ${count}`)
    .join("\n");
  return (
    <Link
      to={toAddress(first.entry.path, "open-questions")}
      className="plan-attention"
      title={tooltip}
    >
      <span className="plan-attention__dot" aria-hidden="true" />
      <span className="plan-attention__name">
        <SentenceText text={shortTitle(first.entry)} />
        {docs.length > 1 && ` and ${docs.length - 1} more`}
      </span>{" "}
      <span className="plan-attention__count">{words}</span>
    </Link>
  );
}

/** A ring filled by done over total in the finished tone, over a hairline track. */
export function ProgressRing({ done, total }: { done: number; total: number }): JSX.Element {
  const filled = total === 0 ? 0 : Math.min(1, done / total);
  return (
    <svg viewBox="0 0 40 40" className="plan-ring" aria-hidden="true">
      <circle className="plan-ring__track" cx="20" cy="20" r="16" />
      {filled > 0 && (
        <circle
          className="plan-ring__fill"
          cx="20"
          cy="20"
          r="16"
          pathLength="1"
          strokeDasharray={`${filled} 1`}
          transform="rotate(-90 20 20)"
        />
      )}
    </svg>
  );
}
