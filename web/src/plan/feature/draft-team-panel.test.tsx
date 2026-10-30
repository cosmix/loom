import { cleanup, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import hello from "@/plan/api/fixtures/hello.json";
import { checklistViewSchema } from "@/plan/api/schema";
import { projectAtom } from "@/plan/state/atoms";
import { paths, renderPanel, session, storeFor, teamView } from "./draft-team-panel-kit";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("DraftTeamPanel", () => {
  it("renders no duplicate offers before a draft team starts", () => {
    const store = storeFor(null);
    const checklist = checklistViewSchema.parse(hello.checklists[0]);
    store.set(projectAtom, { checklists: [checklist], standalone: [], moves: [] });
    const { container } = renderPanel(store, "demo");
    expect(container.innerHTML).toBe("");
  });

  it("hides a team on its own feature during another active session", () => {
    renderPanel(storeFor(teamView(), { ...session, id: "vision-session" }));

    expect(screen.queryByRole("region", { name: "Draft team" })).toBeNull();
  });

  it.each([
    [[paths[0]], true],
    [["doc/plans/feature/PLAN-one.md"], true],
    [[paths[0], "doc/plans/feature/PLAN-one.md"], true],
    [["doc/product/feature-other/section_01.md"], false],
    [["doc/plans/other/PLAN-one.md"], false],
    [["doc/product/other/section_01.md", paths[0]], true],
    [[], false],
  ] as const)("scopes an active team's documents %j to the feature", (documents, visible) => {
    const team = { ...teamView(), documents: [...documents] };
    renderPanel(storeFor(team));

    expect(screen.queryByRole("region", { name: "Draft team" }) !== null).toBe(visible);
  });

  it.each([null, "ended", "failed"] as const)(
    "hides the panel when the matching session is %s",
    (state) => {
      renderPanel(storeFor(teamView(), state === null ? null : { ...session, state }));

      expect(screen.queryByRole("region", { name: "Draft team" })).toBeNull();
    },
  );

  it.each([
    ["a", false, true],
    ["b", false, false],
    ["a", true, false],
    ["b", true, false],
  ] as const)("scopes team on %s with shared offer %s", (feature, shared, visible) => {
    const checklist = checklistViewSchema.parse(hello.checklists[0]);
    const primary = {
      id: "draft-all-plans",
      kind: "draft_all_plans" as const,
      label: "Draft all missing plans",
      target: null,
    };
    const empty = { ...teamView(), documents: [], teammates: [] };
    const store = storeFor(empty, { ...session, offer: primary.id });
    const guided = {
      ...primary,
      id: "write-plan:one",
      kind: "write_plan" as const,
      label: "Write plan one",
    };
    checklist.items = [{ step: 4, text: "Missing plan", path: null, offer: guided }];
    store.set(projectAtom, {
      checklists: [
        { ...checklist, feature: "a", primary: { ...primary, feature: "a" } },
        { ...checklist, feature: "b", primary: shared ? { ...primary, feature: "b" } : null },
      ],
      standalone: [],
      moves: [],
    });
    renderPanel(store, feature);
    expect(screen.queryByRole("region", { name: "Draft team" }) !== null).toBe(visible);
    expect(screen.queryAllByRole("button").map((button) => button.textContent)).toEqual(
      visible ? ["Stop the team"] : [],
    );
  });

  it("shows the ordered documents with their owners and three queue states", () => {
    renderPanel();

    const panel = screen.getByRole("region", { name: "Draft team" });
    expect(within(panel).getByRole("heading", { name: "Drafting with a team" }).tagName).toBe("H2");
    const queue = screen.getByRole("list", { name: "Document queue" });
    expect(queue.tagName).toBe("OL");
    const rows = within(queue).getAllByRole("listitem");
    expect(rows).toHaveLength(3);
    expect(within(rows[0]).getByText("Ada")).toBeTruthy();
    expect(within(rows[0]).getByText("Drafted")).toBeTruthy();
    expect(within(rows[1]).getByText("Ada")).toBeTruthy();
    expect(within(rows[1]).getByText("Drafting")).toBeTruthy();
    expect(within(rows[2]).getByText("Grace")).toBeTruthy();
    expect(within(rows[2]).getByText("Queued")).toBeTruthy();
    expect(within(rows[0]).getByRole("link", { name: paths[0] }).getAttribute("href")).toBe(
      "/files/product/feature/section_01.md",
    );
    expect(rows.map((row) => within(row).getByRole("link").textContent)).toEqual(paths);
    expect(queue.querySelectorAll('[data-kind="drafting"]')).toHaveLength(1);
    expect(rows[1].querySelector('[data-kind="drafting"]')).not.toBeNull();
    expect(queue.querySelectorAll(".lucide-book-open")).toHaveLength(3);
  });

  it("shows all token counts including cached tokens without a tooltip", () => {
    renderPanel();

    expect(screen.getByText("2 teammates · 3 documents · 5.0k tokens so far")).toBeTruthy();
    const teammates = screen.getByRole("list", { name: "Teammates" });
    const rows = within(teammates).getAllByRole("listitem");
    expect(rows).toHaveLength(2);
    expect(within(rows[0]).getByText("Ada")).toBeTruthy();
    expect(within(rows[0]).getByText("writing")).toBeTruthy();
    expect(within(rows[0]).getByText("§02")).toBeTruthy();
    expect(within(rows[0]).getByText(paths[1])).toBeTruthy();
    expect(within(rows[0]).getByText("1.9k")).toBeTruthy();
    expect(
      within(rows[0]).getByText("1.0k in / 400 out · 200 cache write · 300 cache read"),
    ).toBeTruthy();
    expect(within(rows[1]).getByText("Grace")).toBeTruthy();
    expect(within(rows[1]).getByText("3.1k")).toBeTruthy();
    expect(within(rows[1]).queryByText(/^§/)).toBeNull();
    expect(within(rows[1]).getByText("Current document: None")).toBeTruthy();
    expect(
      within(rows[1]).getByText("2.0k in / 500 out · 100 cache write · 500 cache read"),
    ).toBeTruthy();
    expect(teammates.querySelector("[title]")).toBeNull();
  });

  it.each([
    ["working", null, "Queued"],
    ["working", "doc/product/unknown.md", "Queued"],
    ["idle", null, "Drafted"],
    ["idle", paths[0], "Queued"],
    ["idle", paths[1], "Drafted"],
  ] as const)("shows %s with current %s as %s", (state, current, expected) => {
    const team = teamView();
    team.teammates[0] = { ...team.teammates[0], state, current };
    renderPanel(storeFor(team));

    const queue = screen.getByRole("list", { name: "Document queue" });
    const row = within(queue).getAllByRole("listitem")[0];
    expect(within(row).getByText(expected)).toBeTruthy();
    expect(row.querySelector('[data-kind="drafting"]')).toBeNull();
    const teammates = screen.getByRole("list", { name: "Teammates" });
    const card = within(teammates).getAllByRole("listitem")[0];
    expect(within(card).getByText(state === "working" ? "writing" : "drafted")).toBeTruthy();
  });

  it("uses teammate assignment order while preserving the document queue order", () => {
    const team = teamView();
    team.documents = [paths[1], paths[0], paths[2]];
    renderPanel(storeFor(team));

    const queue = screen.getByRole("list", { name: "Document queue" });
    const rows = within(queue).getAllByRole("listitem");
    expect(within(rows[0]).getByRole("link", { name: paths[1] })).toBeTruthy();
    expect(within(rows[0]).getByText("Drafting")).toBeTruthy();
    expect(within(rows[1]).getByText("Drafted")).toBeTruthy();
  });

  it("keeps unassigned documents queued", () => {
    renderPanel(storeFor({ ...teamView(), teammates: [] }));

    const queue = screen.getByRole("list", { name: "Document queue" });
    expect(within(queue).getAllByText("Unassigned")).toHaveLength(3);
    expect(within(queue).getAllByText("Queued")).toHaveLength(3);
  });

  it("isolates server names and paths and encodes file links", () => {
    const path = "doc/plans/feature/PLAN-team #1?\u202E.md";
    const team = teamView();
    team.documents = [path];
    team.teammates = [
      { ...team.teammates[0], name: "Draft\u202Eteam", documents: [path], current: path },
    ];
    renderPanel(storeFor(team));

    const link = screen.getByRole("link", { name: "doc/plans/feature/PLAN-team #1?⟨U+202E⟩.md" });
    expect(link.getAttribute("href")).toBe("/files/plans/feature/PLAN-team%20%231%3F%E2%80%AE.md");
    expect(screen.getAllByText("Draft⟨U+202E⟩team")).toHaveLength(2);
    expect(screen.getAllByText("doc/plans/feature/PLAN-team #1?⟨U+202E⟩.md")).toHaveLength(2);
    const queue = screen.getByRole("list", { name: "Document queue" });
    expect(queue.querySelector(".lucide-list-checks")).not.toBeNull();
  });
});
