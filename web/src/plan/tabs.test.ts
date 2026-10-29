import { describe, expect, it } from "vitest";
import { FileMode } from "./files/file-mode";
import { isPlanTabActive, PLAN_TABS, PLAN_TAB_ROUTES, planTabPath } from "./tabs";

describe("plan tabs", () => {
  it("registers the Files tab with file mode", () => {
    expect(PLAN_TABS).toEqual([
      { id: "product", label: "Product", path: null },
      { id: "engineering", label: "Engineering", path: null },
      { id: "files", label: "Files", path: "/files", element: FileMode },
    ]);
    expect(PLAN_TAB_ROUTES.map((route) => route.path)).toEqual(["/files/*"]);
  });

  it("resolves concept targets for a feature and preserves Files", () => {
    expect(PLAN_TABS.map((tab) => planTabPath(tab, "demo"))).toEqual([
      "/f/demo/product",
      "/f/demo/engineering",
      "/files",
    ]);
    expect(PLAN_TABS.map((tab) => planTabPath(tab, null))).toEqual([null, null, "/files"]);
  });

  it.each([
    ["/f/demo/product", "product"],
    ["/f/other/product/vision", "product"],
    ["/f/demo/engineering", "engineering"],
    ["/f/other/engineering/implementation", "engineering"],
    ["/plans/standalone", "engineering"],
    ["/files/docs/intro.md", "files"],
    ["/f/demo/productivity", null],
    ["/plans-extra/standalone", null],
    ["/", null],
  ])("matches the view for %s", (pathname, active) => {
    expect(PLAN_TABS.filter((tab) => isPlanTabActive(tab, pathname)).map((tab) => tab.id)).toEqual(
      active === null ? [] : [active],
    );
  });
});
