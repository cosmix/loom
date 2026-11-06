import { cleanup, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { mount, path, resetNet, respond, turn } from "./turn-footer-kit";

vi.mock("./restore", () => ({
  RestoreButton: () => null,
  RestoreAll: () => null,
}));

beforeEach(() => {
  resetNet();
  localStorage.clear();
  vi.clearAllMocks();
  vi.stubGlobal("fetch", vi.fn(respond));
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("unroutable summary ids render as plain text", () => {
  mount(turn({ changed_files: [], summaries_rebuilding: ["//evil.example/page", "/../x"] }));
  expect(screen.getByText("//evil.example/page")).toBeTruthy();
  expect(screen.getByText("/../x")).toBeTruthy();
  expect(screen.queryByRole("link")).toBeNull();
});

it.each([
  [path, "section"],
  ["doc/product/demo/index.md", "vision"],
  ["doc/plans/demo/index.md", "metaplan"],
  ["doc/plans/demo/metaplan-02.md", null],
  ["doc/plans/demo/PLAN-draft.md", "plan"],
  ["doc/plans/demo/PLAN-metaplan.md", "plan"],
  ["doc/plans/archive/DONE-PLAN-built.md", "plan"],
  ["doc/plans/demo/briefs/draft/common.md", "brief"],
  ["doc/loom/knowledge/patterns/chat.md", "knowledge"],
  ["doc/notes.md", null],
  ["doc/plans/demo/REVIEW-run-PLAN-draft.md", null],
  ["web/src/index.md", null],
] as const)("changed file %s has document mark %s", async (name, kind) => {
  mount(turn({ changed_files: [{ path: name, before_hash: "h", by_tool: true }] }));
  const row = screen.getByTitle(name.replace(/^doc\//, "")).closest("li")!;
  const mark = row.querySelector("[data-doc-kind]");
  expect(mark?.getAttribute("data-doc-kind") ?? null).toBe(kind);
  if (kind !== null) expect(mark?.querySelector("svg")).toBeTruthy();
  if (name.startsWith("doc/")) await screen.findByText("+1 −1");
});
