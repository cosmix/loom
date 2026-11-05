import { atom } from "jotai/vanilla";

import type { ChecklistView, StandalonePlanView } from "@/plan/api/schema";
import { featureOfDocPath } from "@/plan/feature/source-href";
import { parseAddress } from "@/plan/files/address";

export type ViewedKind =
  | "vision"
  | "product-overview"
  | "section"
  | "metaplan"
  | "engineering-overview"
  | "plan"
  | "run"
  | "file"
  | null;

export type ViewedDocument = {
  kind: ViewedKind;
  /** Repo-relative source path to attach, or null when the page has none (run). */
  path: string | null;
  /** Short name for the chip: "section_05", "Product overview", "metaplan". */
  name: string;
};

export const NO_DOCUMENT: ViewedDocument = { kind: null, path: null, name: "" };

type Standalone = Pick<StandalonePlanView, "slug" | "path">;

/** What the routed page shows, written by `ScreenSync`. */
export const viewedDocumentAtom = atom<ViewedDocument>(NO_DOCUMENT);

/**
 * The document the user dismissed the context line for, null while none is. `path` is null for a
 * page without a source file (run), which no other page shares. Only `ScreenSync`
 * clears it, on a route change: the dismissal follows the document, so another URL for the same
 * file keeps it and a checklist arriving on the same page does not bring the line back.
 */
export const contextDismissedAtom = atom<{ path: string | null } | null>(null);

/** The document the chat treats as context: the viewed one, unless it was dismissed. */
export const contextDocumentAtom = atom<ViewedDocument>((get) =>
  get(contextDismissedAtom) !== null ? NO_DOCUMENT : get(viewedDocumentAtom),
);

function segmentsOf(pathname: string): string[] | null {
  try {
    return pathname.split("/").filter(Boolean).map(decodeURIComponent);
  } catch {
    return null;
  }
}

/** A decoded segment that could climb out of its folder or split into two. */
function unsafeSegment(segment: string): boolean {
  return (
    segment === "." ||
    segment === ".." ||
    segment.includes("/") ||
    segment.includes("\\") ||
    segment.includes("\0")
  );
}

/** `/files/<path relative to doc/>`; null for anything that does not name a file under `doc/`. */
function fileOfPath(pathname: string): ViewedDocument {
  const segments = segmentsOf(pathname);
  if (segments === null || segments[0] !== "files" || segments.length < 2) return NO_DOCUMENT;
  if (segments.slice(1).some(unsafeSegment)) return NO_DOCUMENT;
  const { path } = parseAddress({ pathname, search: "", hash: "" });
  return { kind: "file", path, name: segments.at(-1)! };
}

/**
 * `/f/<feature>/source/<path relative to doc/>`: the same file `/files/<path>` names, kept inside
 * its feature. Only a path the source page itself shows counts (`featureOfDocPath`), so a path
 * under another feature or outside the feature layout is no document here either.
 */
function sourceOf(feature: string, rest: string[]): ViewedDocument {
  if (rest.length === 0 || rest.some(unsafeSegment)) return NO_DOCUMENT;
  const path = `doc/${rest.join("/")}`;
  if (featureOfDocPath(path) !== feature) return NO_DOCUMENT;
  return { kind: "file", path, name: rest.at(-1)! };
}

/**
 * The checklist belongs to the page only when it is the page's own feature: `checklistAtom` keeps
 * the last visited feature's checklist while the route has already moved to another one, and a
 * path read from it would attach the wrong feature's document.
 */
function ownChecklist(feature: string, checklist: ChecklistView | null): ChecklistView | null {
  return checklist !== null && checklist.feature === feature ? checklist : null;
}

function metaplanOf(checklist: ChecklistView | null): { path: string | null; name: string } {
  return { path: checklist?.metaplans[0]?.path ?? null, name: "metaplan" };
}

/*
 * The page helpers below agree with the pages themselves (feature/document-page.tsx): with
 * the feature's own checklist loaded, a page that has no document gives NO_DOCUMENT, so the chat
 * shows no line and no prompts for a source it could not attach. With no checklist (or another
 * feature's) the name stays and the path is null, because the page may yet turn out to have one.
 */
function productPage(segments: string[], own: ChecklistView | null): ViewedDocument {
  const doc = segments[3];
  if (doc === undefined) {
    if (own && !own.has_product) return NO_DOCUMENT;
    const path = own ? `doc/product/${own.feature}/index.md` : null;
    return { kind: "product-overview", path, name: "Product overview" };
  }
  const number = doc.startsWith("section_") ? doc.slice("section_".length) : null;
  const written = own?.sections.some((item) => item.number === number && item.written);
  if (own && !written) return NO_DOCUMENT;
  return { kind: "section", path: own ? `doc/product/${own.feature}/${doc}.md` : null, name: doc };
}

function engineeringPage(segments: string[], own: ChecklistView | null): ViewedDocument {
  const doc = segments[3];
  if (doc === undefined) {
    const { path } = metaplanOf(own);
    if (own && path === null) return NO_DOCUMENT;
    return { kind: "engineering-overview", path, name: "Engineering overview" };
  }
  const row = own?.plan_rows.find((item) => item.slug === doc);
  if (own && !row) return NO_DOCUMENT;
  return { kind: "plan", path: row?.path ?? null, name: doc };
}

/** Pages under `/f/:feature/<page>`; `segments` is the whole decoded pathname. */
function featurePageOf(segments: string[], checklist: ChecklistView | null): ViewedDocument {
  const [, feature, page, ...rest] = segments;
  if (feature === undefined || page === undefined) return NO_DOCUMENT;
  if (page === "source") return sourceOf(feature, rest);
  if (rest.length > 1) return NO_DOCUMENT;
  const own = ownChecklist(feature, checklist);
  switch (page) {
    case "vision":
      if (rest.length > 0 || (own && !own.has_product)) return NO_DOCUMENT;
      return {
        kind: "vision",
        path: own ? `doc/product/${own.feature}/index.md` : null,
        name: "vision",
      };
    case "product":
      return productPage(segments, own);
    case "metaplan": {
      if (rest.length > 0) return NO_DOCUMENT;
      const metaplan = metaplanOf(own);
      return own && metaplan.path === null ? NO_DOCUMENT : { kind: "metaplan", ...metaplan };
    }
    case "engineering":
      return engineeringPage(segments, own);
    case "run":
      if (rest.length > 0) return NO_DOCUMENT;
      return { kind: "run", path: null, name: "Run" };
    default:
      return NO_DOCUMENT;
  }
}

/**
 * The document a route shows, and the source file to attach for it. Total: an unknown route, a
 * missing checklist, a document the checklist does not list or a malformed escape gives a value
 * with no kind or no path, never an error. A route that needs the checklist for its path keeps its
 * name while the checklist is missing or belongs to another feature; once the feature's own
 * checklist is loaded and lists no such document, the route has no document at all.
 */
export function viewedOfPath(
  pathname: string,
  checklist: ChecklistView | null,
  standalone: readonly Standalone[] = [],
): ViewedDocument {
  const segments = segmentsOf(pathname);
  if (segments === null) return NO_DOCUMENT;
  const [route, name, ...rest] = segments;
  if (route === "f") return featurePageOf(segments, checklist);
  if (route === "plans" && name !== undefined && rest.length === 0) {
    const plan = standalone.find((item) => item.slug === name);
    return { kind: "plan", path: plan?.path ?? null, name };
  }
  return route === "files" ? fileOfPath(pathname) : NO_DOCUMENT;
}
