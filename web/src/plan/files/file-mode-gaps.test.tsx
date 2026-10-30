import { screen } from "@testing-library/react";
import { createStore } from "jotai";
import { describe, expect, it, vi } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { checklistViewSchema } from "@/plan/api/schema";
import type { DocView } from "@/plan/api/schema-files";
import { activeFeatureAtom, checklistAtom, projectAtom } from "@/plan/state/atoms";

import { renderMode, tree } from "./file-mode-kit";

vi.mock("@/plan/files/api", () => ({
  fetchDoc: vi.fn(),
  fetchTree: vi.fn(),
  searchDocs: vi.fn(),
  useFileSocket: () => {},
}));
vi.mock("@/plan/files/plan/plan-view", () => ({
  PlanView: ({ doc }: { doc: DocView }) => <div data-testid="plan-view">{doc.plan?.slug}</div>,
}));

describe("file mode layout", () => {
  it.each([
    ["section 03: Map view", "doc/product/demo/section_03.md", "write_section", "Write section 03"],
    [
      "plan map-view named in doc/plans/demo/index.md is not written",
      "doc/plans/demo/PLAN-map-view.md",
      "write_plan",
      "Write plan map-view",
    ],
  ] as const)(
    "reloading the gap for %s finds its offer without an active feature",
    (text, path, kind, label) => {
      const store = createStore();
      const checklist = checklistViewSchema.parse(structuredClone(checklistFixture.checklists[0]));
      const item = {
        step: kind === "write_section" ? 2 : 4,
        text,
        path,
        offer: { id: `write:${path}`, kind, label, target: path, feature: "demo" },
      };
      const other = {
        ...item,
        offer: { ...item.offer, feature: "other", label: "Write other Map view" },
      };
      store.set(projectAtom, {
        checklists: [
          { ...checklist, feature: "other", items: [other] },
          { ...checklist, items: [item] },
        ],
        standalone: [],
        moves: [],
      });
      expect(store.get(activeFeatureAtom)).toBeNull();
      expect(store.get(checklistAtom)).toBeNull();
      renderMode(
        `/files?gap=Map%20view&feature=demo&target=${encodeURIComponent(path)}`,
        tree(),
        store,
      );
      expect(screen.getByRole("heading", { name: "Map view" })).toBeTruthy();
      expect(screen.getByRole("navigation", { name: "Breadcrumb" }).textContent).toBe(
        "DemoMap view",
      );
      expect(screen.getByText("This document is missing.")).toBeTruthy();
      expect(screen.getByText(text)).toBeTruthy();
      expect(screen.getByText(label)).toBeTruthy();
      expect(screen.queryByText("Write other Map view")).toBeNull();
      expect(screen.queryByText("No checklist item asks for it yet.")).toBeNull();
    },
  );

  it("shows no checklist item when a link has no target and an item has no path", () => {
    const store = createStore();
    const checklist = checklistViewSchema.parse(structuredClone(checklistFixture.checklists[0]));
    const item = { ...checklist.items[1]!, text: "Map view", path: null };
    store.set(projectAtom, {
      checklists: [{ ...checklist, items: [item] }],
      standalone: [],
      moves: [],
    });

    renderMode("/files?gap=Map%20view&feature=demo", tree(), store);

    expect(screen.getByText("No checklist item asks for it yet.")).toBeTruthy();
    expect(screen.queryByText(item.offer?.label ?? "")).toBeNull();
  });
});
