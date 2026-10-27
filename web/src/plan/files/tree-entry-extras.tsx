import type { JSX } from "react";

import type { TreeEntry } from "@/plan/api/schema-files";

/**
 * Extension slot at the end of every tree row. Plan 9 fills it with the
 * "changed since you looked" dot; until then there is nothing to show.
 */
export function TreeEntryExtras(_props: { entry: TreeEntry }): JSX.Element | null {
  return null;
}
