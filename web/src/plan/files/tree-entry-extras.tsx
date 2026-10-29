import type { JSX } from "react";

import type { TreeEntry } from "@/plan/api/schema-files";
import { repoRel } from "@/plan/files/address";
import { useChangedSince } from "@/plan/review/changed-since";
import { OpenQuestionCount } from "@/plan/review/open-questions";

export function TreeEntryExtras({ entry }: { entry: TreeEntry }): JSX.Element | null {
  const changes = useChangedSince();
  const entryPath = entry.path;
  if (entryPath === null) return null;
  const changed = changes.some(({ path }) => path === repoRel(entryPath));
  return (
    <span className="inline-flex flex-wrap items-center gap-1.5 text-xs [&_a]:text-xs [&_a]:whitespace-nowrap">
      {changed && (
        <span
          role="img"
          aria-label="changed since you looked"
          title="changed since you looked"
          className="shrink-0 text-[12px] text-muted-foreground"
        >
          <span aria-hidden="true">●</span>
        </span>
      )}
      <OpenQuestionCount path={entryPath} />
    </span>
  );
}
