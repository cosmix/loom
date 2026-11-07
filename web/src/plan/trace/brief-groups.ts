import type { BundleLink } from "./model";

// A plan's brief bundle as the briefs card draws it: one group per folder under the plan's
// `briefs/<slug>/` directory, the files at its top first. Pure; `briefs-card.tsx` renders it.

export interface Brief {
  path: string;
  to: string;
  /** The file's name (`c1-wire.md`). */
  name: string;
  /** The name a chip shows: a Markdown brief without its `.md`. */
  stem: string;
}

export interface BriefGroup {
  /** The folder under the bundle (`chat-server`, `build/r1`); "" for the bundle's own files. */
  folder: string;
  briefs: Brief[];
}

/** A bundle this long starts folded behind its count, so the heading stays short. */
export const BRIEFS_FOLDED_OVER = 6;

const ORDER = new Intl.Collator("en", { numeric: true });

function briefOf(link: BundleLink): { folder: string; brief: Brief } {
  const cut = link.label.lastIndexOf("/");
  const name = link.label.slice(cut + 1);
  return {
    folder: cut < 0 ? "" : link.label.slice(0, cut),
    brief: { path: link.path, to: link.to, name, stem: name.replace(/\.md$/i, "") },
  };
}

/** The bundle by folder: the top-level files, then each folder, in natural order (c2 before c10). */
export function briefGroups(links: readonly BundleLink[]): BriefGroup[] {
  const groups = new Map<string, Brief[]>();
  for (const link of links) {
    const { folder, brief } = briefOf(link);
    groups.set(folder, [...(groups.get(folder) ?? []), brief]);
  }
  return [...groups.entries()]
    .sort(([a], [b]) => (a === "" ? -1 : b === "" ? 1 : ORDER.compare(a, b)))
    .map(([folder, briefs]) => ({
      folder,
      briefs: briefs.sort((a, b) => ORDER.compare(a.name, b.name)),
    }));
}
