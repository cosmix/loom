import { createStore } from "jotai";
import { afterEach, describe, expect, it, vi } from "vitest";
import fixture from "@/plan/api/fixtures/files-doc-plan.json";
import { docViewSchema } from "@/plan/api/schema-files";
import { docViewAtom, passageOf, refreshAllDocViews, refreshDocView } from "./passages";

const doc = docViewSchema.parse({
  ...fixture,
  text: "Intro\n\n## Parent\nParent text\n### Child\nChild text\n## Next\nNext text",
  sections: [
    { level: 2, title: "Parent", anchor: "parent", start_line: 3, end_line: 6 },
    { level: 3, title: "Child", anchor: "child", start_line: 5, end_line: 6 },
    { level: 2, title: "Next", anchor: "next", start_line: 7, end_line: 8 },
  ],
});

afterEach(() => vi.unstubAllGlobals());

describe("passageOf", () => {
  it("cuts a nested section using its inclusive line range", () => {
    expect(passageOf(doc, "child")).toEqual({
      title: "Child",
      anchor: "child",
      startLine: 5,
      endLine: 6,
      markdown: "### Child\nChild text",
      hash: `${doc.hash}#child`,
    });
    expect(passageOf(doc, "parent")?.markdown).toBe(
      "## Parent\nParent text\n### Child\nChild text",
    );
  });

  it("cuts the preamble before the first heading", () => {
    expect(passageOf(doc, "")).toEqual({
      title: doc.title,
      anchor: "",
      startLine: 1,
      endLine: 2,
      markdown: "Intro\n",
      hash: `${doc.hash}#`,
    });
  });

  it("leaves a leading front matter block out of the preamble", () => {
    const fronted = docViewSchema.parse({
      ...fixture,
      text: "---\nkind: document\ncites:\n  a: b\n---\n\nIntro\n## Parent\nParent text",
      sections: [{ level: 2, title: "Parent", anchor: "parent", start_line: 8, end_line: 9 }],
    });
    expect(passageOf(fronted, "")).toMatchObject({
      startLine: 6,
      endLine: 7,
      markdown: "\nIntro",
    });
  });

  it("keeps a preamble whose opening rule never closes", () => {
    const open = docViewSchema.parse({ ...fixture, text: "---\nIntro", sections: [] });
    expect(passageOf(open, "")?.markdown).toBe("---\nIntro");
  });

  it("returns null for a missing anchor", () => {
    expect(passageOf(doc, "missing")).toBeNull();
  });
});

describe("passageOf on a plan with a metadata block", () => {
  const text = [
    "# Plan: X",
    "",
    "## Overview",
    "Does x.",
    "",
    "## Residual risks",
    "Risk one.",
    "",
    "---",
    "",
    "<!-- loom METADATA -->",
    "",
    "```yaml",
    "loom:",
    "  version: 2",
    "```",
    "",
    "<!-- END loom METADATA -->",
    "",
    "## Pressure test",
    "Pressure notes.",
  ].join("\n");
  const plan = (sections: object[], body = text) =>
    docViewSchema.parse({
      ...fixture,
      text: body,
      sections,
      plan: { ...fixture.plan, metadata_line: 11 },
    });
  const sections = [
    { level: 2, title: "Overview", anchor: "overview", start_line: 3, end_line: 5 },
    { level: 2, title: "Residual risks", anchor: "residual-risks", start_line: 6, end_line: 19 },
    { level: 2, title: "Pressure test", anchor: "pressure-test", start_line: 20, end_line: 21 },
  ];

  it("ends the section that runs through the block where the block begins", () => {
    const risks = passageOf(plan(sections), "residual-risks");
    expect(risks).toMatchObject({
      startLine: 6,
      endLine: 7,
      markdown: "## Residual risks\nRisk one.",
    });
    expect(risks?.markdown).not.toMatch(/METADATA|loom:|version/);
  });

  it("leaves sections before and after the block whole", () => {
    expect(passageOf(plan(sections), "overview")?.markdown).toBe("## Overview\nDoes x.\n");
    expect(passageOf(plan(sections), "pressure-test")).toMatchObject({
      startLine: 20,
      endLine: 21,
      markdown: "## Pressure test\nPressure notes.",
    });
  });

  it("ends a preamble that runs to the end of the file where the block begins", () => {
    const body = ["Intro text.", "", "<!-- loom METADATA -->", "```yaml", "loom: {}", "```"];
    const bare = docViewSchema.parse({
      ...fixture,
      text: body.join("\n"),
      sections: [],
      plan: { ...fixture.plan, metadata_line: 3 },
    });
    expect(passageOf(bare, "")).toMatchObject({
      startLine: 1,
      endLine: 1,
      markdown: "Intro text.",
    });
  });
});

describe("document cache", () => {
  it("memoizes each path and refreshes one subscribed document", async () => {
    const store = createStore();
    const updated = { ...doc, hash: "updated" };
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(new Response(JSON.stringify(doc)))
      .mockResolvedValueOnce(new Response(JSON.stringify(updated)));
    vi.stubGlobal("fetch", fetchMock);
    const view = docViewAtom("doc/product/a & b.md");
    expect(docViewAtom("doc/product/a & b.md")).toBe(view);
    const notify = vi.fn();
    const unsubscribe = store.sub(view, notify);
    expect(await store.get(view)).toEqual(doc);
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/doc?path=product%2Fa%20%26%20b.md",
      expect.anything(),
    );
    expect(await store.get(view)).toEqual(doc);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    refreshDocView(store, "doc/product/a & b.md");
    expect(await store.get(view)).toEqual(updated);
    expect(notify).toHaveBeenCalled();
    expect(fetchMock).toHaveBeenCalledTimes(2);
    unsubscribe();
  });

  it("refreshes all loaded documents in the supplied store", async () => {
    const store = createStore();
    let version = "initial";
    const fetchMock = vi
      .fn()
      .mockImplementation(() =>
        Promise.resolve(new Response(JSON.stringify({ ...doc, hash: version }))),
      );
    vi.stubGlobal("fetch", fetchMock);
    const first = docViewAtom("doc/product/refresh-all-one.md");
    const second = docViewAtom("doc/product/refresh-all-two.md");
    const unsubFirst = store.sub(first, () => {});
    const unsubSecond = store.sub(second, () => {});
    expect((await store.get(first)).hash).toBe("initial");
    expect((await store.get(second)).hash).toBe("initial");
    version = "new";
    refreshAllDocViews(store);
    expect((await store.get(first)).hash).toBe("new");
    expect((await store.get(second)).hash).toBe("new");
    expect(fetchMock).toHaveBeenCalledTimes(4);
    unsubFirst();
    unsubSecond();
  });
});
