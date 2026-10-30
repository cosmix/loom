import { describe, expect, it } from "vitest";
import { FileMode } from "./files/file-mode";
import { isPlanTabActive, PLAN_TABS, PLAN_TAB_ROUTES } from "./tabs";

describe("plan tabs", () => {
  it("registers only the Files tab, with file mode", () => {
    expect(PLAN_TABS).toEqual([{ id: "files", label: "Files", path: "/files", element: FileMode }]);
    expect(PLAN_TAB_ROUTES.map((route) => route.path)).toEqual(["/files/*"]);
  });

  it.each([
    ["/files", "files"],
    ["/files/docs/intro.md", "files"],
    ["/f/demo/product", null],
    ["/f/demo/engineering", null],
    ["/plans/standalone", null],
    ["/filesystem", null],
    ["/", null],
  ])("matches the view for %s", (pathname, active) => {
    expect(PLAN_TABS.filter((tab) => isPlanTabActive(tab, pathname)).map((tab) => tab.id)).toEqual(
      active === null ? [] : [active],
    );
  });
});
