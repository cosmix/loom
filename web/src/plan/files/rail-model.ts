import type { TreeEntry } from "@/plan/api/schema-files";
import { repoRel } from "@/plan/files/address";
import { isFinished, liftedChildren } from "@/plan/files/tree-model";

/** One block of the Plans tab: a feature (or a metaplan outside one) and its plans in order. */
export interface RailSection {
  key: string;
  /** The feature's slug, a stray metaplan's title, or "Standalone plans". */
  name: string;
  metaplan: TreeEntry | null;
  /** Plans and not-yet-written metaplan rows, in the metaplan's order. */
  plans: TreeEntry[];
}

const isRailRow = (entry: TreeEntry) => entry.kind === "plan" || entry.kind === "metaplan_entry";

/**
 * The Plans category as rail sections: one per feature, one per metaplan outside a feature, and
 * one for standalone plans. Anything else listed there comes back in `rest`, to be drawn as rows.
 */
export function railSections(entries: TreeEntry[]): { sections: RailSection[]; rest: TreeEntry[] } {
  const sections: RailSection[] = [];
  const standalone: TreeEntry[] = [];
  const rest: TreeEntry[] = [];
  for (const entry of entries) {
    if (entry.kind === "feature") {
      const lifted = liftedChildren(entry);
      sections.push({
        key: `feature:${entry.title}`,
        name: entry.feature ?? entry.title,
        metaplan: lifted.find((child) => child.kind === "metaplan") ?? null,
        plans: lifted.filter(isRailRow),
      });
    } else if (entry.kind === "metaplan") {
      sections.push({
        key: entry.path ?? entry.title,
        name: entry.title,
        metaplan: { ...entry, children: [] },
        plans: entry.children.filter(isRailRow),
      });
    } else if (isRailRow(entry)) {
      standalone.push(entry);
    } else {
      rest.push(entry);
    }
  }
  if (standalone.length > 0) {
    sections.push({
      key: "standalone",
      name: "Standalone plans",
      metaplan: null,
      plans: standalone,
    });
  }
  return { sections, rest };
}

/** "11 plans complete", or "4 of 11 plans complete" while some are not. */
export function completeText(plans: TreeEntry[]): { done: number; total: number; text: string } {
  const done = plans.filter(isFinished).length;
  const total = plans.length;
  const noun = total === 1 ? "plan" : "plans";
  const text =
    done === total ? `${total} ${noun} complete` : `${done} of ${total} ${noun} complete`;
  return { done, total, text };
}

/** How a plan's node is drawn on the rail; the first that applies wins. */
export type RailNode = "gap" | "error" | "question" | "running" | "done" | "retired" | "written";

export function railNode(entry: TreeEntry, errors: number, questions: number): RailNode {
  if (entry.gap || entry.kind === "metaplan_entry") return "gap";
  if (errors > 0) return "error";
  if (questions > 0) return "question";
  if (entry.state === "running") return "running";
  if (entry.state === "done") return "done";
  if (entry.state === "retired") return "retired";
  return "written";
}

/** A node that asks for the reader: its plan starts open on the rail. */
export function needsAttention(node: RailNode): boolean {
  return node === "error" || node === "question" || node === "running";
}

/** A plan's stage names from its brief bundle: the stage folders, then any loose stage brief. */
export function stageNames(plan: TreeEntry): string[] {
  return plan.children.flatMap((child) => {
    if (child.kind === "brief_folder") return [child.title];
    if (child.kind === "brief" && child.file_name !== null && child.file_name !== "common.md") {
      return [child.file_name.replace(/\.md$/i, "")];
    }
    return [];
  });
}

/** Every document under `entries` (themselves included) with open questions, in tree order. */
export function questionDocs(
  entries: TreeEntry[],
  counts: Record<string, number>,
): { entry: TreeEntry; count: number }[] {
  return entries.flatMap((entry) => {
    const count = entry.path === null ? 0 : (counts[repoRel(entry.path)] ?? 0);
    const own = count > 0 ? [{ entry, count }] : [];
    return [...own, ...questionDocs(entry.children, counts)];
  });
}
