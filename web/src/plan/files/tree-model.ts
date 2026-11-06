import type { TreeEntry } from "@/plan/api/schema-files";

/** The slug in a plan file name, whatever its lock prefix: `IN_PROGRESS-PLAN-x.md` gives `x`. */
export function planSlug(fileName: string | null): string | null {
  const found = /^(?:(?:IN_PROGRESS|DONE|RETIRED)-)?PLAN-(.+)\.md$/i.exec(fileName ?? "");
  return found ? found[1].toLowerCase() : null;
}

/** A plan whose work is over. The tree folds these under one "Finished plans" row. */
export function isFinished(entry: TreeEntry): boolean {
  return entry.kind === "plan" && (entry.state === "done" || entry.state === "retired");
}

/** An entry's children, a feature's metaplan made a leaf with its rows lifted after it in order. */
export function liftedChildren(entry: TreeEntry): TreeEntry[] {
  if (entry.kind !== "feature") return entry.children;
  return entry.children.flatMap((child) =>
    child.kind === "metaplan" ? [{ ...child, children: [] }, ...child.children] : [child],
  );
}

/**
 * The rows drawn under an entry, with its finished plans set apart for the caller to fold.
 * A feature's metaplan is drawn as a leaf and its plans are lifted beside it: the feature row
 * already names their scope, and the extra level only took width from every title under it.
 */
export function arrangeChildren(entry: TreeEntry): { rows: TreeEntry[]; finished: TreeEntry[] } {
  const lifted = liftedChildren(entry);
  return {
    rows: lifted.filter((child) => !isFinished(child)),
    finished: lifted.filter(isFinished),
  };
}

/** Every plan a feature holds, through its metaplan. */
function featurePlans(entry: TreeEntry): TreeEntry[] {
  const { rows, finished } = arrangeChildren(entry);
  return [...rows, ...finished].filter(
    (child) => child.kind === "plan" || child.kind === "metaplan_entry",
  );
}

/**
 * The count a feature row carries: "<finished>/<listed> done" over its plans, gaps included;
 * for a product feature, how many of its documents are missing. Null when there is nothing to
 * count.
 */
export function featureCount(entry: TreeEntry): { text: string; spoken: string } | null {
  if (entry.kind !== "feature") return null;
  const plans = featurePlans(entry);
  if (plans.length > 0) {
    const finished = plans.filter(isFinished).length;
    return {
      text: `${finished}/${plans.length}`,
      spoken: `${finished} of ${plans.length} plans finished`,
    };
  }
  const missing = entry.children.filter((child) => child.gap).length;
  return missing > 0 ? { text: `${missing} missing`, spoken: `${missing} missing` } : null;
}

const PLAN_PREFIX = /^plan(?::|\s+[-–—])\s*/i;
const STAGE_PREFIX = /^stage(?::|\s+[-–—])?\s+/i;

/** `title` without `prefix`, or the whole title when nothing would be left. */
function without(title: string, prefix: RegExp): string {
  const rest = title.replace(prefix, "");
  return rest.trim() === "" ? title : rest;
}

/** A plan slug after its feature's prefix: `feat-x-02-server` in `feat-x` gives `02-server`. */
function featureRest(slug: string, feature: string | null): string | null {
  const prefix = feature === null ? null : `${feature.toLowerCase()}-`;
  return prefix !== null && slug.startsWith(prefix) ? slug.slice(prefix.length) : null;
}

/**
 * The title a row shows, in the same inline Markdown as the full one. The tree already says
 * what a row is (its mark, its parent), so the words that repeat it are dropped: "Plan:" before a
 * plan, "Stage" before a stage brief, a feature's own slug before its plan's slug, and the
 * vision's, the metaplan's and the common brief's long headings. The full title stays on the row's tooltip.
 */
export function shortTitle(entry: TreeEntry): string {
  if (entry.kind === "vision" && entry.feature !== null) return "Vision";
  if (entry.kind === "metaplan" && entry.feature !== null) return "Metaplan";
  if (entry.kind === "brief" && entry.file_name === "common.md") return "Common brief";
  if (entry.kind === "brief") return without(entry.title, STAGE_PREFIX);
  if (entry.kind !== "plan") return entry.title;
  const rest = featureRest(entry.title.toLowerCase(), entry.feature);
  if (rest !== null && entry.title.toLowerCase() === planSlug(entry.file_name)) {
    return without(rest, /^\d+-/);
  }
  return without(entry.title, PLAN_PREFIX);
}

/**
 * The ordinal set in mono before a row's title: a feature plan's from its slug (`feat-x-02-server`
 * gives `02`), a product section's from its file name (`section_03.md` gives `03`).
 */
export function entryNumber(entry: TreeEntry): string | null {
  if (entry.kind === "section")
    return /^section_(\d+)\.md$/.exec(entry.file_name ?? "")?.[1] ?? null;
  if (entry.kind !== "plan") return null;
  const slug = planSlug(entry.file_name);
  const rest = slug === null ? null : featureRest(slug, entry.feature);
  return rest === null ? null : (/^(\d+)-/.exec(rest)?.[1] ?? null);
}
