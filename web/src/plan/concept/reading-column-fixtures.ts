import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import filesDoc from "@/plan/api/fixtures/files-doc.json";
import {
  conceptPageSchema,
  type ConceptPage,
  type Mark,
  type Sentence,
} from "@/plan/api/schema-concept";

/** Builders shared by the reading column tests. */
export const S1 = "doc/product/section_01.md";
export const S2 = "doc/product/section_02.md";
export const TEXT = "# Doc\n\n## Intro\nAlpha text\n\n## Detail\nBeta text\n";
const SECTIONS = [
  { level: 2, title: "Intro", anchor: "intro", start_line: 3, end_line: 5 },
  { level: 2, title: "Detail", anchor: "detail", start_line: 6, end_line: 8 },
];

export const docLink = (stem: string, title: string) => ({
  id: stem,
  title,
  path: `doc/product/${stem}.md`,
  page: `product/${stem}`,
});
export const mark = (
  stem: string,
  title: string,
  colour: number,
  out: Mark["out_of_date"] = null,
): Mark => ({
  doc: docLink(stem, title),
  colour,
  out_of_date: out,
});
export const src = (path: string, anchor: string) => ({ kind: "source" as const, path, anchor });
export const cite = (summary: string, id: string) => ({ kind: "sentence" as const, summary, id });

export function sent(
  id: string,
  text: string,
  citations: Sentence["citations"] = [],
  marks: Mark[] = [],
) {
  const rests_on = citations.flatMap((c) =>
    c.kind === "source" ? [{ path: c.path, anchor: c.anchor }] : [],
  );
  return { id, text, marks, citations, rests_on, plans: [] } satisfies Sentence;
}

export function pageOf(
  key: string,
  sentences: Sentence[],
  extra: Record<string, unknown> = {},
): ConceptPage {
  const base = structuredClone(summaryFixture);
  const blocks = [{ kind: "paragraph", sentences }];
  return conceptPageSchema.parse({ ...base, key, key_chips: [], blocks, ...extra });
}

export function docOf(path: string, text = TEXT, hash = "h1") {
  return { ...structuredClone(filesDoc), path, title: "Doc", text, hash, sections: SECTIONS };
}

export const rect = (top: number) =>
  ({
    top,
    bottom: top + 20,
    left: 0,
    right: 0,
    width: 0,
    height: 20,
    x: 0,
    y: top,
    toJSON() {},
  }) as DOMRect;
