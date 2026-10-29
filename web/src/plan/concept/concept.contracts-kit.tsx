import summaryFixture from "@/plan/api/fixtures/concept-summary.json";

export type Anchor = { path: string; anchor: string; title?: string };
export type Page = Record<string, unknown>;

export function clone<T>(value: T): T {
  return structuredClone(value);
}

export function sentence(id: string, text: string, restsOn: Anchor[]) {
  const citations = restsOn.map(({ path, anchor }) => ({ kind: "source", path, anchor }));
  return { id, text, marks: [], citations, rests_on: restsOn, plans: [] };
}

export function summaryPage(key: string, sentences: ReturnType<typeof sentence>[]): Page {
  const heading = { kind: "heading", level: 1, text: "Planning" };
  return { ...clone(summaryFixture), key, blocks: [heading, { kind: "paragraph", sentences }] };
}
