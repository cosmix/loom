import { describe, expect, it } from "vitest";
import {
  type PageKeyParts,
  docKindOfPageKey,
  featureOfKey,
  isConceptPageKey,
  isOverviewKey,
  isPageDoc,
  overviewKey,
  pageHref,
  pageKeyOfPath,
  pageKeyOfSummaryId,
  parsePageKey,
  scopeOfKey,
  summaryKey,
} from "./page-key";

const pages: { key: string; parts: PageKeyParts; href: string }[] = [
  {
    key: "overview/demo/product",
    parts: { kind: "overview", feature: "demo", scope: "product" },
    href: "/f/demo/product",
  },
  {
    key: "demo/product/section_01",
    parts: { kind: "summary", feature: "demo", scope: "product", doc: "section_01" },
    href: "/f/demo/product/section_01",
  },
  {
    key: "demo/product/vision",
    parts: { kind: "summary", feature: "demo", scope: "product", doc: "vision" },
    href: "/f/demo/vision",
  },
  {
    key: "demo/engineering/feat-x.v2",
    parts: { kind: "summary", feature: "demo", scope: "engineering", doc: "feat-x.v2" },
    href: "/f/demo/engineering/feat-x.v2",
  },
  {
    key: "demo/metaplan/index",
    parts: { kind: "summary", feature: "demo", scope: "metaplan", doc: "index" },
    href: "/f/demo/metaplan",
  },
  {
    key: "plans/standalone",
    parts: { kind: "standalone", doc: "standalone" },
    href: "/plans/standalone",
  },
];

describe("concept page keys", () => {
  it.each(pages)("parses and formats $key", ({ key, parts }) => {
    expect(parsePageKey(key)).toEqual(parts);
    expect(isConceptPageKey(key)).toBe(true);
    if (parts.kind === "overview") {
      expect(overviewKey(parts.feature, parts.scope)).toBe(key);
    } else if (parts.kind === "summary") {
      expect(summaryKey(parts.feature, parts.scope, parts.doc)).toBe(key);
    } else {
      expect(summaryKey(null, "engineering", parts.doc)).toBe(key);
    }
  });

  it.each([
    "",
    "product",
    "product/x",
    "overview/Bad/product",
    "overview/f/other",
    "f/product/..",
    `overview/${"a".repeat(65)}/product`,
    "plans/a/b",
    "overview/plans/product",
    "overview/overview/product",
    "overview/archive/product",
    "overview/briefs/product",
    "f/product/",
    "f/product/a\n",
    "plans/.",
    "plans/..",
    "plans/a b",
    `plans/${"a".repeat(129)}`,
    "demo/metaplan/Index",
    "demo/metaplan/-x",
    "overview/demo/metaplan",
    ["overview", "demo", "engineering"].join("/"),
  ])("rejects invalid key %j", (key) => {
    expect(parsePageKey(key)).toBeNull();
    expect(docKindOfPageKey(key)).toBeNull();
    expect(isConceptPageKey(key)).toBe(false);
    expect(isOverviewKey(key)).toBe(false);
    expect(scopeOfKey(key)).toBeNull();
    expect(featureOfKey(key)).toBeNull();
    expect(pageHref(key)).toBe("/");
  });

  it("accepts the maximum feature and document lengths", () => {
    expect(parsePageKey(`${"a".repeat(64)}/product/${"b".repeat(128)}`)).toEqual({
      kind: "summary",
      feature: "a".repeat(64),
      scope: "product",
      doc: "b".repeat(128),
    });
  });

  it.each(["x", "A-z_09.v2", "...", "_", "a".repeat(128)])("accepts doc %j", (doc) => {
    expect(isPageDoc(doc)).toBe(true);
  });

  it.each(["", ".", "..", "a/b", "a b", "a%2Fb", "é", "a\n", "a".repeat(129)])(
    "rejects doc %j",
    (doc) => {
      expect(isPageDoc(doc)).toBe(false);
    },
  );

  it.each([
    { key: "overview/demo/product", overview: true, scope: "product", feature: "demo" },
    { key: "demo/product/section_01", overview: false, scope: "product", feature: "demo" },
    { key: "demo/engineering/plan", overview: false, scope: "engineering", feature: "demo" },
    { key: "plans/plan", overview: false, scope: "engineering", feature: null },
    { key: "demo/metaplan/index", overview: false, scope: "metaplan", feature: "demo" },
  ])("reads metadata for $key", ({ key, overview, scope, feature }) => {
    expect(isOverviewKey(key)).toBe(overview);
    expect(scopeOfKey(key)).toBe(scope);
    expect(featureOfKey(key)).toBe(feature);
  });

  it.each([
    ["overview/demo/product", "vision"],
    ["demo/product/section_01", "section"],
    ["demo/product/vision", "vision"],
    ["demo/engineering/plan", "plan"],
    ["demo/metaplan/index", "metaplan"],
    ["plans/standalone", "plan"],
  ])("maps %s to document kind %s", (key, kind) => {
    expect(docKindOfPageKey(key)).toBe(kind);
  });
});

describe("concept page URLs", () => {
  it("uses the metaplan for the plans area", () => {
    expect(overviewKey("demo", "engineering")).toBe("demo/metaplan/index");
    expect(pageHref(overviewKey("demo", "engineering"))).toBe("/f/demo/metaplan");
  });
  it.each(pages)("round trips $key through $href", ({ key, href }) => {
    expect(pageHref(key)).toBe(href);
    expect(pageKeyOfPath(href)).toBe(key);
    expect(pageKeyOfPath(pageHref(key))).toBe(key);
  });

  it.each([
    ["/f/%64emo/%70roduct", "overview/demo/product"],
    ["/f/%64emo/vision", "demo/product/vision"],
    ["/f/demo/product/section%5F01", "demo/product/section_01"],
    ["/plans/feat%2Dx%2Ev2", "plans/feat-x.v2"],
  ])("decodes segments in %s", (pathname, key) => {
    expect(pageKeyOfPath(pathname)).toBe(key);
  });

  it.each([
    "/files/x",
    "/f/demo",
    "/new",
    "/",
    "f/demo/product",
    "/f/demo/other",
    "/f/demo/engineering",
    "/f/demo/product/",
    "/f/demo/product/a/b",
    "/plans/a/b",
    "/plans/%",
    "/plans/%E0%A4%A",
    "/plans/a%2Fb",
    "/plans/%2E%2E",
    "/plans/a%252Fb",
    "/f/Bad/product",
    "/f/demo/metaplan/index.md",
    "/f/demo/metaplan/index",
    "/f/demo/metaplan/a/b",
  ])("rejects pathname %j", (pathname) => {
    expect(pageKeyOfPath(pathname)).toBeNull();
  });
});

describe("summary ids", () => {
  it.each([
    "overview/demo/product",
    "demo/product/section_01",
    "demo/product/section_123",
    "demo/product/vision",
    "demo/metaplan/index",
  ])("preserves summary id %s", (id) => {
    expect(pageKeyOfSummaryId(id, () => null)).toBe(id);
  });

  it("rejects a metaplan summary id other than index", () => {
    expect(parsePageKey("demo/metaplan/metaplan-02")).toBeNull();
    expect(pageKeyOfSummaryId("demo/metaplan/metaplan-02", () => null)).toBeNull();
  });

  it("uses a plan's feature for its engineering page", () => {
    const planFeature = (slug: string) => (slug === "feat-x.v2" ? "demo" : null);

    expect(pageKeyOfSummaryId("plans/feat-x.v2", planFeature)).toBe("demo/engineering/feat-x.v2");
  });

  it("preserves a standalone plan summary id", () => {
    expect(pageKeyOfSummaryId("plans/standalone", () => null)).toBe("plans/standalone");
  });

  it("rejects an invalid feature returned for a plan", () => {
    expect(pageKeyOfSummaryId("plans/plan", () => "Bad")).toBeNull();
  });

  it.each([
    "product",
    "product/x",
    "overview/Bad/product",
    "demo/engineering/plan",
    "demo/product/section_1",
    "demo/product/section_01x",
    "demo/product/other",
    "plans/..",
    "plans/a..b",
    "plans/_plan",
    "plans/a/b",
  ])("rejects summary id %j before resolving a feature", (id) => {
    expect(
      pageKeyOfSummaryId(id, () => {
        throw new Error("invalid ids must not resolve a feature");
      }),
    ).toBeNull();
  });
});
