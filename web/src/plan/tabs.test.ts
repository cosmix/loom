import { describe, expect, it } from "vitest";
import { FileMode } from "./files/file-mode";
import { PLAN_TABS, PLAN_TAB_ROUTES } from "./tabs";

describe("plan tabs", () => {
  it("registers the Files tab with file mode", () => {
    expect(PLAN_TABS).toEqual([{ id: "files", label: "Files", path: "/files", element: FileMode }]);
    expect(PLAN_TAB_ROUTES.map((route) => route.path)).toEqual(["/files/*"]);
  });
});
