import { atom, type Atom } from "jotai";
import { useAtomValue } from "jotai/react";
import { unwrap } from "jotai/utils";
import { useMemo } from "react";

import { apiGet } from "@/plan/api/client";
import { docViewSchema, type DocView } from "@/plan/api/schema-files";
import type { TraceDoc, TraceView } from "@/plan/api/schema-trace";
import { docRel } from "@/plan/files/address";
import type { CoverLookup, CoverSection } from "@/plan/markdown/covers";
import { withoutNumber } from "@/plan/trace/model";
import { useTrace } from "@/plan/trace/state";

/** A section file's heading titles by anchor, every level, or "failed" when it could not be read. */
export type Outline = ReadonlyMap<string, string> | "failed";

const outlines = new Map<string, { body: string; atom: Atom<Outline | null> }>();
const lastOutline = new Map<string, Outline>();

async function fetchOutline(path: string): Promise<Outline> {
  try {
    const url = `/api/doc?path=${encodeURIComponent(docRel(path))}`;
    const result = await apiGet(url, docViewSchema);
    if (!result.ok) return "failed";
    return new Map(result.value.sections.map((section) => [section.anchor, section.title]));
  } catch {
    return "failed";
  }
}

/**
 * The outline of a section at one version of its text (the trace's body hash), null while the
 * first one loads. Keyed by the hash, so an edit to the section fetches it again; the outline
 * last read stands in while the next one loads, so the titles never fall back to slugs.
 */
function outlineAtom(path: string, body: string): Atom<Outline | null> {
  const held = outlines.get(path);
  if (held?.body === body) return held.atom;
  const fetched = atom(async () => {
    const outline = await fetchOutline(path);
    lastOutline.set(path, outline);
    return outline;
  });
  const entry = { body, atom: unwrap(fetched, () => lastOutline.get(path) ?? null) };
  outlines.set(path, entry);
  return entry.atom;
}

function sectionDocs(trace: TraceView): TraceDoc[] {
  return trace.documents.filter((doc) => doc.role === "section" && doc.number !== null);
}

/**
 * The sections whose outline has to be read: the trace lists only level-2 headings, so a section
 * is read when an entry's `Covers:` names one of its anchors the trace does not list.
 */
export function sectionsToRead(trace: TraceView): TraceDoc[] {
  const listed = new Set(trace.headings.map((heading) => `${heading.path}#${heading.anchor}`));
  const unlisted = new Set(
    trace.entries.flatMap((entry) =>
      entry.covers
        .filter((node) => node.anchor !== null && !listed.has(`${node.path}#${node.anchor}`))
        .map((node) => node.path),
    ),
  );
  return sectionDocs(trace).filter((doc) => doc.progress !== "stub" && unlisted.has(doc.path));
}

/**
 * The feature's sections by number, each with its heading titles: the trace's level-2 headings,
 * then the read outline's. A section waiting on its outline is not complete, so a heading only
 * the outline can name reads as pending, not as missing.
 */
export function buildCoverLookup(
  trace: TraceView,
  read: ReadonlyMap<string, Outline | null>,
): CoverLookup {
  const byNumber = new Map<number, CoverSection>();
  for (const doc of sectionDocs(trace)) {
    const number = doc.number ?? 0;
    if (byNumber.has(number)) continue;
    const headings = new Map<string, string>();
    for (const heading of trace.headings) {
      if (heading.path === doc.path) headings.set(heading.anchor, heading.title);
    }
    const outline = read.get(doc.path);
    if (outline && outline !== "failed") {
      for (const [anchor, title] of outline) headings.set(anchor, title);
    }
    const title = withoutNumber(doc.title, number);
    byNumber.set(number, { path: doc.path, title, headings, complete: outline !== null });
  }
  return (number) => byNumber.get(number) ?? null;
}

/**
 * The lookup a metaplan's `Covers:` lines resolve through, or null for any other document and
 * until the feature's trace has loaded (the lines then read as the prose they are).
 */
export function useCoverLookup(doc: DocView): CoverLookup | null {
  const feature = doc.kind === "metaplan" ? (doc.feature ?? "") : "";
  const { trace } = useTrace(feature);
  const combined = useMemo(() => {
    const atoms = (trace ? sectionsToRead(trace) : []).map(
      (section) => [section.path, outlineAtom(section.path, section.current.body)] as const,
    );
    return atom((get) => new Map(atoms.map(([path, outline]) => [path, get(outline)])));
  }, [trace]);
  const read = useAtomValue(combined);
  return useMemo(() => (trace ? buildCoverLookup(trace, read) : null), [trace, read]);
}
