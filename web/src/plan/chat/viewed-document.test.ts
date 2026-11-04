import { createStore } from "jotai/vanilla";
import { expect, it } from "vitest";

import type { ChecklistView } from "@/plan/api/schema";
import { checklist, section } from "./context-kit";
import {
  contextDismissedAtom,
  contextDocumentAtom,
  NO_DOCUMENT,
  viewedDocumentAtom,
  viewedOfPath,
} from "./viewed-document";

const view = (pathname: string, list: ChecklistView | null = checklist()) =>
  viewedOfPath(pathname, list, [{ slug: "solo", path: "doc/plans/PLAN-solo.md" }]);

it.each([
  ["/f/atlas/vision", "vision", "doc/product/atlas/index.md", "vision"],
  ["/f/atlas/product", "product-overview", "doc/product/atlas/index.md", "Product overview"],
  ["/f/atlas/product/section_05", "section", "doc/product/atlas/section_05.md", "section_05"],
  ["/f/atlas/metaplan", "metaplan", "doc/plans/atlas/index.md", "metaplan"],
  [
    "/f/atlas/metaplan/second-round.md",
    "metaplan",
    "doc/plans/atlas/second-round.md",
    "second-round",
  ],
  [
    "/f/atlas/engineering",
    "engineering-overview",
    "doc/plans/atlas/index.md",
    "Engineering overview",
  ],
  [
    "/f/atlas/engineering/atlas-01-documents",
    "plan",
    "doc/plans/atlas/IN_PROGRESS-PLAN-atlas-01-documents.md",
    "atlas-01-documents",
  ],
  ["/f/atlas/pressure", "pressure", null, "Pressure test"],
  ["/f/atlas/run", "run", null, "Run"],
  ["/plans/solo", "plan", "doc/plans/PLAN-solo.md", "solo"],
  ["/files/product/atlas/notes.md", "file", "doc/product/atlas/notes.md", "notes.md"],
] as const)("%s is a %s with source %s", (pathname, kind, path, name) => {
  expect(view(pathname)).toEqual({ kind, path, name });
});

it("percent-encoded segments are decoded, for the feature and for a file", () => {
  const spaced = checklist({ feature: "at las", sections: [section("05")] });
  expect(view("/f/at%20las/product/section_05", spaced).path).toBe(
    "doc/product/at las/section_05.md",
  );
  expect(view("/f/at%20las/vision", spaced).path).toBe("doc/product/at las/index.md");
  expect(view("/files/product/at%20las/note%20one.md")).toEqual({
    kind: "file",
    path: "doc/product/at las/note one.md",
    name: "note one.md",
  });
});

it("a section the loaded checklist does not list, or has not written, has no document", () => {
  expect(view("/f/atlas/product/section_06")).toEqual(NO_DOCUMENT);
  expect(view("/f/atlas/product/section_99")).toEqual(NO_DOCUMENT);
  expect(view("/f/atlas/product/notes")).toEqual(NO_DOCUMENT);
});

it("a plan or metaplan the loaded checklist does not list has no document", () => {
  expect(view("/f/atlas/engineering/atlas-09-missing")).toEqual(NO_DOCUMENT);
  expect(view("/f/atlas/metaplan/gone.md")).toEqual(NO_DOCUMENT);
});

it("a standalone plan nobody lists keeps its name and no path", () => {
  expect(view("/plans/unknown")).toEqual({ kind: "plan", path: null, name: "unknown" });
});

it("a feature with no vision or metaplan has no document on its overview pages", () => {
  const bare = checklist({ has_product: false, metaplans: [], sections: [] });
  expect(view("/f/atlas/vision", bare)).toEqual(NO_DOCUMENT);
  expect(view("/f/atlas/product", bare)).toEqual(NO_DOCUMENT);
  expect(view("/f/atlas/engineering", bare)).toEqual(NO_DOCUMENT);
  expect(view("/f/atlas/metaplan", bare)).toEqual(NO_DOCUMENT);
});

it("pressure and run have no document by design but keep their names with a loaded checklist", () => {
  const bare = checklist({ has_product: false, metaplans: [], sections: [] });
  expect(view("/f/atlas/pressure", bare)).toEqual({
    kind: "pressure",
    path: null,
    name: "Pressure test",
  });
  expect(view("/f/atlas/run", bare)).toEqual({ kind: "run", path: null, name: "Run" });
});

it.each([
  "/f/atlas/vision",
  "/f/atlas/product",
  "/f/atlas/product/section_05",
  "/f/atlas/metaplan",
  "/f/atlas/engineering",
  "/f/atlas/engineering/atlas-01-documents",
])("with no checklist %s keeps its kind and name and has no path", (pathname) => {
  const shown = view(pathname, null);
  expect(shown.kind).not.toBeNull();
  expect(shown.name).not.toBe("");
  expect(shown.path).toBeNull();
});

it.each([
  "/f/beta/vision",
  "/f/beta/product",
  "/f/beta/product/section_05",
  "/f/beta/metaplan",
  "/f/beta/engineering",
  "/f/beta/engineering/atlas-01-documents",
])("%s reads no path from the checklist of another feature", (pathname) => {
  // checklistAtom keeps the last visited feature's checklist while the route has moved on.
  const shown = view(pathname, checklist());
  expect(shown.kind).not.toBeNull();
  expect(shown.path).toBeNull();
});

it.each([
  "",
  "/",
  "/new",
  "/settings",
  "/plans",
  "/f",
  "/f/atlas",
  "/f/atlas/",
  "/f/atlas/unknown",
  "/f/atlas/vision/extra",
  "/f/atlas/product/section_05/extra",
  "/f/atlas/run/extra",
  "/files",
  "/files/",
  "/files/%2e%2e/secret.md",
  "/files/product/..%2F..%2Fsecret.md",
  "/files/product/a%5Cb.md",
  "/files/%E0%A4%A",
  "/f/%E0%A4%A/vision",
])("%j is not a document and does not throw", (pathname) => {
  expect(view(pathname)).toEqual(NO_DOCUMENT);
});

it("a feature's source page is the same file document as its /files address", () => {
  const source = "/f/demo/source/product/demo/section_01.md";
  expect(view(source)).toEqual({
    kind: "file",
    path: "doc/product/demo/section_01.md",
    name: "section_01.md",
  });
  expect(view(source)).toEqual(view("/files/product/demo/section_01.md"));
  expect(view("/f/demo/source/plans/demo/index.md").path).toBe("doc/plans/demo/index.md");
  expect(view("/f/atlas/source/product/atlas/note%20one.md")).toEqual({
    kind: "file",
    path: "doc/product/atlas/note one.md",
    name: "note one.md",
  });
});

it.each([
  "/f/demo/source",
  "/f/demo/source/",
  "/f/demo/source/product/demo/..%2Fother/x.md",
  "/f/demo/source/product/demo/a%5Cb.md",
  "/f/demo/source/product/demo/../x.md",
  "/f/demo/source/x",
  "/f/demo/source/product/other/section_01.md",
  "/f/demo/source/plans/PLAN-solo.md",
  "/f/demo/source/%E0%A4%A",
])("the source page %j is not a document", (pathname) => {
  expect(view(pathname)).toEqual(NO_DOCUMENT);
});

it("a file address never leaves doc/", () => {
  for (const pathname of ["/files/doc/product/x.md", "/files/product/x.md", "/files/x.md"]) {
    expect(view(pathname).path).toMatch(/^doc\/[^.]/);
  }
});

it("the context is the viewed document until it is dismissed", () => {
  const store = createStore();
  const shown = view("/f/atlas/product/section_05");
  store.set(viewedDocumentAtom, shown);
  expect(store.get(contextDocumentAtom)).toEqual(shown);
  store.set(contextDismissedAtom, { path: shown.path });
  expect(store.get(contextDocumentAtom)).toEqual(NO_DOCUMENT);
  expect(store.get(viewedDocumentAtom)).toEqual(shown);
});
