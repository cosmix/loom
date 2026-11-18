import { act, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { EntryKind } from "@/plan/api/schema-files";

import { authorStore, makeDoc, mount } from "./doc-heading-kit";
import { registerDocHeadingHooks, seedSummary } from "./doc-heading-view.fixtures";

registerDocHeadingHooks();

describe("FileDocHeading", () => {
  it("links a plan's Summary and marks its file Source as current", () => {
    const doc = makeDoc({ feature: "demo" }, { slug: "file-mode" });
    const store = authorStore();
    seedSummary(store, doc, "current");
    mount(doc, store);
    const views = screen.getByRole("group", { name: "View" });

    expect(within(views).getByRole("link", { name: "Summary" }).getAttribute("href")).toBe(
      "/f/demo/engineering/file-mode",
    );
    const source = within(views).getByRole("link", { name: "Source" });
    expect(source.getAttribute("href")).toBe("/f/demo/source/plans/demo/PLAN-file-mode.md");
    expect(source.getAttribute("aria-current")).toBe("page");
  });

  it.each<[EntryKind, string | null, string | null]>([
    ["section", "demo", "/f/demo/product/section_03"],
    ["vision", "demo", "/f/demo/vision"],
    ["metaplan", "demo", "/f/demo/metaplan"],
    ["plan", null, "/plans/file-mode"],
    ["section", null, null],
    ["vision", null, null],
    ["metaplan", null, null],
    ["note", "demo", null],
  ])("links the Summary for kind=%s feature=%s", (kind, feature, target) => {
    const doc = makeDoc(
      {
        kind,
        feature,
        path: kind === "metaplan" ? "doc/plans/demo/index.md" : "doc/product/demo/section_03.md",
        ...(kind === "plan" ? {} : { plan: null }),
      },
      { slug: "file-mode" },
    );
    const store = authorStore();
    if (target !== null) seedSummary(store, doc, "current");
    mount(doc, store);
    const views = screen.getByRole("group", { name: "View" });

    if (target === null) {
      expect(within(views).queryByRole("link", { name: "Summary" })).toBeNull();
      expect(within(views).getByText("Summary").getAttribute("aria-disabled")).toBe("true");
    } else {
      expect(within(views).getByRole("link", { name: "Summary" }).getAttribute("href")).toBe(
        target,
      );
    }
  });

  it("disables Summary with its reason until the summary exists, then enables it live", () => {
    const doc = makeDoc({
      feature: "demo",
      kind: "section",
      path: "doc/product/demo/section_03.md",
      plan: null,
    });
    const store = authorStore();
    mount(doc, store);
    const views = screen.getByRole("group", { name: "View" });

    // No page yet, then a page whose summary file is absent: both leave Summary blocked.
    const blocked = within(views).getByText("Summary");
    expect(blocked.getAttribute("aria-disabled")).toBe("true");
    expect(blocked.getAttribute("title")).toBe("This document has no summary yet");
    act(() => seedSummary(store, doc, "missing"));
    expect(within(views).queryByRole("link", { name: "Summary" })).toBeNull();

    act(() => seedSummary(store, doc, "current"));
    const link = within(views).getByRole("link", { name: "Summary" });
    expect(link.getAttribute("href")).toBe("/f/demo/product/section_03");
    expect(link.getAttribute("aria-disabled")).toBeNull();
  });
});
