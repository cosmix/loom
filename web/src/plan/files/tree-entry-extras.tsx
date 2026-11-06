import type { JSX } from "react";

import type { TreeEntry } from "@/plan/api/schema-files";
import { repoRel } from "@/plan/files/address";
import { useChangedSince } from "@/plan/review/changed-since";
import { OpenQuestionCount } from "@/plan/review/open-questions";

/** The review marks at a row's end: its open questions, then a dot if it changed since you looked. */
export function TreeEntryExtras({ entry }: { entry: TreeEntry }): JSX.Element | null {
  const changes = useChangedSince();
  const entryPath = entry.path;
  if (entryPath === null) return null;
  const changed = changes.some(({ path }) => path === repoRel(entryPath));
  return (
    <>
      <OpenQuestionCount path={entryPath} />
      {changed && (
        <span
          role="img"
          aria-label="changed since you looked"
          title="changed since you looked"
          className="plan-tree__changed"
        />
      )}
    </>
  );
}
