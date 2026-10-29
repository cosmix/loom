import { useAtomValue } from "jotai/react";
import { useId } from "react";
import { Link } from "react-router";

import type { ChecklistView } from "@/plan/api/schema";
import { Untrusted } from "@/plan/chat/visible-controls";
import { treeAtom } from "@/plan/files/atoms";
import { treeDocs } from "@/plan/files/tree";
import { ProgressStrip } from "@/plan/home/progress-strip";
import { featuresAtom } from "@/plan/state/atoms";

export function FeatureCard({ checklist }: { checklist: ChecklistView }) {
  const tree = useAtomValue(treeAtom);
  const headingId = useId();
  const visionPath = `doc/product/${checklist.feature}/index.md`;
  const pitch = tree ? treeDocs(tree).find((entry) => entry.path === visionPath)?.blurb : null;
  const done = checklist.steps.filter((step) => step.mark === "done").length;
  const total = checklist.steps.length;
  const gaps = checklist.items.length;

  return (
    <Link
      to={`/f/${encodeURIComponent(checklist.feature)}`}
      aria-labelledby={headingId}
      className="flex h-full min-w-0 flex-col gap-4 rounded-xl border border-hairline bg-card p-5 text-card-foreground hover:border-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background"
    >
      <h2 id={headingId} className="font-display text-2xl leading-tight font-semibold break-words">
        <Untrusted text={checklist.feature} />
      </h2>
      {pitch && (
        <p className="text-sm leading-relaxed break-words text-muted-foreground">
          <Untrusted text={pitch} />
        </p>
      )}
      <div className="mt-auto space-y-3">
        <ProgressStrip steps={checklist.steps} />
        <div className="flex flex-wrap justify-between gap-x-4 gap-y-1 text-xs text-muted-foreground">
          <span>
            {done} of {total} {total === 1 ? "step" : "steps"} done
          </span>
          <span>
            {gaps} {gaps === 1 ? "gap" : "gaps"}
          </span>
        </div>
      </div>
    </Link>
  );
}

export function FeatureCards() {
  const features = useAtomValue(featuresAtom);
  if (features.length === 0) return null;

  return (
    <ul aria-label="Features" className="grid grid-cols-1 gap-5 sm:grid-cols-2 xl:grid-cols-3">
      {features.map((checklist) => (
        <li key={checklist.feature} className="min-w-0">
          <FeatureCard checklist={checklist} />
        </li>
      ))}
    </ul>
  );
}
