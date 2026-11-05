import type { DocKind } from "@/plan/ui/glyphs";

import { isFeatureName } from "../new/feature-name";

/** The document name of the vision's summary: key `<feature>/product/vision`, page `/f/<feature>/vision`. */
export const VISION_DOC = "vision";

/** The document family of a feature's concept page. */
export type ConceptScope = "product" | "engineering" | "metaplan";

/** The summary stem of a feature's first metaplan (`doc/plans/<feature>/index.md`), served at `/f/<feature>/metaplan`. */
export const FIRST_METAPLAN = "index";

/** A metaplan summary's stem: the metaplan file's name without `.md`, as the server names it. */
const METAPLAN_STEM = /^[a-z0-9][a-z0-9-]*$/;

/** The validated components of a concept page key. */
export type PageKeyParts =
  | { kind: "overview"; feature: string; scope: ConceptScope }
  | { kind: "summary"; feature: string; scope: ConceptScope; doc: string }
  | { kind: "standalone"; doc: string };

/** Mirror the server's safe_doc rule for a document name. */
export function isPageDoc(doc: string): boolean {
  return (
    doc.length >= 1 &&
    doc.length <= 128 &&
    !/[^A-Za-z0-9._-]/.test(doc) &&
    doc !== "." &&
    doc !== ".."
  );
}

/** Parse the feature-qualified and standalone keys accepted by PageKey::parse. */
export function parsePageKey(key: string): PageKeyParts | null {
  const parts = key.split("/");
  const [first = "", second = "", third = ""] = parts;
  if (parts.length === 2 && first === "plans" && isPageDoc(second)) {
    return { kind: "standalone", doc: second };
  }
  if (parts.length !== 3) return null;
  if (first === "overview" && isFeatureName(second)) {
    if (third === "product" || third === "engineering") {
      return { kind: "overview", feature: second, scope: third };
    }
    return null;
  }
  if (isFeatureName(first) && isPageDoc(third)) {
    if (second === "product" || second === "engineering") {
      return { kind: "summary", feature: first, scope: second, doc: third };
    }
    if (second === "metaplan" && METAPLAN_STEM.test(third)) {
      return { kind: "summary", feature: first, scope: second, doc: third };
    }
  }
  return null;
}

/** Return the document kind of a valid concept page key. */
export function docKindOfPageKey(key: string): DocKind | null {
  const parts = parsePageKey(key);
  if (parts === null) return null;
  if (parts.kind === "overview") return parts.scope === "product" ? "vision" : "metaplan";
  if (parts.kind === "summary" && parts.scope === "metaplan") return "metaplan";
  if (parts.kind === "summary" && parts.scope === "product") {
    return parts.doc === VISION_DOC ? "vision" : "section";
  }
  return "plan";
}

/** Whether a string identifies a valid concept page. */
export function isConceptPageKey(key: string): boolean {
  return parsePageKey(key) !== null;
}

/** Format an overview key from a validated feature and scope. */
export function overviewKey(feature: string, scope: ConceptScope): string {
  return `overview/${feature}/${scope}`;
}

/** Format a summary key, using plans/<doc> when the feature is absent. */
export function summaryKey(feature: string | null, scope: ConceptScope, doc: string): string {
  return feature === null ? `plans/${doc}` : `${feature}/${scope}/${doc}`;
}

/** Whether a valid page key identifies an overview. */
export function isOverviewKey(key: string): boolean {
  return parsePageKey(key)?.kind === "overview";
}

/** Return the page's scope, with standalone plans belonging to engineering. */
export function scopeOfKey(key: string): ConceptScope | null {
  const parts = parsePageKey(key);
  if (parts === null) return null;
  return parts.kind === "standalone" ? "engineering" : parts.scope;
}

/** Return the owning feature, or null for standalone plans and invalid keys. */
export function featureOfKey(key: string): string | null {
  const parts = parsePageKey(key);
  return parts === null || parts.kind === "standalone" ? null : parts.feature;
}

/** Format a page URL with encoded segments, falling back to / for invalid keys. */
export function pageHref(key: string): string {
  const parts = parsePageKey(key);
  if (parts === null) return "/";
  if (parts.kind === "summary" && parts.scope === "product" && parts.doc === VISION_DOC) {
    return `/f/${encodeURIComponent(parts.feature)}/${VISION_DOC}`;
  }
  if (parts.kind === "summary" && parts.scope === "metaplan") {
    const feature = encodeURIComponent(parts.feature);
    return parts.doc === FIRST_METAPLAN
      ? `/f/${feature}/metaplan`
      : `/f/${feature}/metaplan/${encodeURIComponent(parts.doc)}.md`;
  }
  const segments =
    parts.kind === "standalone"
      ? ["plans", parts.doc]
      : ["f", parts.feature, parts.scope, ...(parts.kind === "summary" ? [parts.doc] : [])];
  return `/${segments.map((segment) => encodeURIComponent(segment)).join("/")}`;
}

/** The key of `/f/<feature>/<page>`: the vision, the first metaplan, or an overview. */
function pageKeyOf4(feature: string, page: string): string {
  if (page === VISION_DOC) return `${feature}/product/${VISION_DOC}`;
  if (page === "metaplan") return `${feature}/metaplan/${FIRST_METAPLAN}`;
  return `overview/${feature}/${page}`;
}

/** Decode a concept page pathname, rejecting other routes and malformed escapes. */
export function pageKeyOfPath(pathname: string): string | null {
  let segments: string[];
  try {
    segments = pathname.split("/").map((segment) => decodeURIComponent(segment));
  } catch (error) {
    if (error instanceof URIError) return null;
    throw error;
  }
  const [root, route, feature = "", scope = "", doc = ""] = segments;
  if (root !== "") return null;
  let key: string;
  if (route === "plans" && segments.length === 3) {
    key = summaryKey(null, "engineering", feature);
  } else if (route === "f" && segments.length === 4) {
    key = pageKeyOf4(feature, scope);
  } else if (route === "f" && segments.length === 5) {
    key = `${feature}/${scope}/${scope === "metaplan" ? doc.replace(/\.md$/, "") : doc}`;
  } else {
    return null;
  }
  return isConceptPageKey(key) ? key : null;
}

/** Map a valid summary-store id to a page key using a plan's owning feature. */
export function pageKeyOfSummaryId(
  id: string,
  planFeature: (slug: string) => string | null,
): string | null {
  const parts = parsePageKey(id);
  if (parts === null) return null;
  if (parts.kind === "overview") return id;
  if (parts.kind === "summary") {
    if (parts.scope === "metaplan") return id;
    const listed = parts.doc === VISION_DOC || /^section_[0-9]{2,}$/.test(parts.doc);
    return parts.scope === "product" && listed ? id : null;
  }
  if (!/^[A-Za-z0-9]/.test(parts.doc) || parts.doc.includes("..")) return null;
  const key = summaryKey(planFeature(parts.doc), "engineering", parts.doc);
  return isConceptPageKey(key) ? key : null;
}
