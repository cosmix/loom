import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { describe, expect, it } from "vitest";

import type { ProjectChecklistView } from "@/plan/api/schema";
import { harness, installStartChatLifecycle } from "@/plan/chat/start-chat-test-kit";
import { standalonePlans } from "@/plan/home/features-contracts-kit";
import { StandaloneShelf } from "@/plan/home/standalone-shelf";
import { authoringAtom, projectAtom } from "@/plan/state/atoms";

installStartChatLifecycle();

const project: ProjectChecklistView = {
  checklists: [],
  standalone: [
    {
      slug: "fix-search",
      path: "doc/plans/PLAN-fix-search.md",
      title: "Fix search",
      state: "written",
      last_change: "2026-11-05T10:00:00Z",
      pressure_date: null,
      has_summary: false,
    },
    {
      slug: "cache",
      path: "doc/plans/IN_PROGRESS-PLAN-cache.md",
      title: "Cache",
      state: "running",
      last_change: "2026-11-06T08:00:00Z",
      pressure_date: null,
      has_summary: false,
    },
    {
      slug: "cleanup",
      path: "doc/plans/DONE-PLAN-cleanup.md",
      title: "Cleanup",
      state: "done",
      last_change: "2026-10-01T12:00:00Z",
      pressure_date: null,
      has_summary: false,
    },
    {
      slug: "old",
      path: "doc/plans/RETIRED-PLAN-old.md",
      title: "Old",
      state: "retired",
      last_change: null,
      pressure_date: null,
      has_summary: false,
    },
  ],
  moves: [],
};

function KickoffProbe() {
  const location = useLocation();
  return <p>Kickoff state: {JSON.stringify(location.state)}</p>;
}

function mount(value = project, readOnly = false) {
  const h = harness();
  h.store.set(projectAtom, value);
  if (readOnly) {
    h.store.set(authoringAtom, {
      holder: { public_id: "other", name: "Alex", away_since: null, frees_at: null },
    });
  }
  h.mount(
    <MemoryRouter>
      <Routes>
        <Route path="/" element={<StandaloneShelf />} />
        <Route path="/new" element={<KickoffProbe />} />
      </Routes>
    </MemoryRouter>,
  );
  return h;
}

const region = () => screen.getByRole("region", { name: "Standalone plans" });
const details = () => region().querySelector("details") as HTMLDetailsElement;
/** The title cells' text, top to bottom. */
const titles = () =>
  within(region())
    .queryAllByRole("link")
    .map((link) => link.textContent);

describe("StandaloneShelf", () => {
  it("lists every plan as a table row: title link, slug and state", () => {
    mount({
      ...project,
      standalone: [
        { ...project.standalone[0]!, path: "doc/plans/PLAN-100%-fix.md" },
        ...project.standalone.slice(1),
      ],
    });

    const table = within(region()).getByRole("table");
    expect(
      within(table)
        .getAllByRole("columnheader")
        .map((cell) => cell.textContent),
    ).toEqual(["", "Plan", "State", "Modified"]);
    expect(screen.getByRole("link", { name: "Fix search" }).getAttribute("href")).toBe(
      "/files/plans/PLAN-100%25-fix.md",
    );
    const cache = screen.getByRole("link", { name: "Cache" });
    expect(cache.getAttribute("href")).toBe("/files/plans/IN_PROGRESS-PLAN-cache.md");
    const row = cache.closest("tr")!;
    expect(row.textContent).toContain("cache");
    expect(within(row).getAllByRole("cell")[2]!.textContent).toBe("running");
    expect(within(row).getByRole("checkbox", { name: "Select Cache" })).toBeTruthy();
  });

  it("opens its table from the first render when the host passes defaultOpen", () => {
    const h = harness();
    h.store.set(projectAtom, project);
    h.mount(
      <MemoryRouter>
        <StandaloneShelf defaultOpen />
      </MemoryRouter>,
    );

    expect(details().open).toBe(true);
  });

  it("starts collapsed, counts the plans in its summary and keeps one action there", () => {
    mount();

    const summary = details().querySelector(":scope > summary") as HTMLElement;
    expect(details().open).toBe(false);
    expect(summary.textContent).toContain("4 standalone plans");
    expect(summary.textContent).toContain("bugfixes, performance work and small features");
    expect(
      within(summary)
        .getAllByRole("button")
        .map((button) => button.textContent),
    ).toEqual(["New standalone plan"]);
    expect(within(summary).queryByRole("link")).toBeNull();
  });

  it("counts the plans in each state under the closed summary, running first", () => {
    mount({
      ...project,
      standalone: [
        ...project.standalone,
        { ...project.standalone[2]!, slug: "b", path: "doc/plans/DONE-PLAN-b.md" },
      ],
    });

    const states = details().querySelector(":scope > summary .shelf__states") as HTMLElement;
    expect([...states.children].map((state) => state.textContent)).toEqual([
      "1 running",
      "1 written",
      "2 done",
      "1 retired",
    ]);
  });

  it("lists the newest plan first, marks Modified as the sorted column and toggles it", () => {
    mount();

    const modified = () => screen.getByRole("columnheader", { name: "Modified" });
    expect(titles()).toEqual(["Cache", "Fix search", "Cleanup", "Old"]);
    expect(modified().getAttribute("aria-sort")).toBe("descending");
    const cell = screen.getByRole("link", { name: "Cleanup" }).closest("tr")!.lastElementChild!;
    const time = cell.querySelector("time")!;
    expect(time.getAttribute("datetime")).toBe("2026-10-01T12:00:00Z");
    expect(time.getAttribute("title")).toBeTruthy();
    expect(
      screen.getByRole("link", { name: "Old" }).closest("tr")!.lastElementChild!.textContent,
    ).toBe("unknown");

    fireEvent.click(screen.getByRole("button", { name: "Modified" }));
    expect(modified().getAttribute("aria-sort")).toBe("ascending");
    // A plan with no readable time stays last in either direction.
    expect(titles()).toEqual(["Cleanup", "Fix search", "Cache", "Old"]);
  });

  it("does not toggle the shelf when the summary's action is clicked", () => {
    // Read-only, so the click stays on this route and only the summary's handling is observed.
    mount(project, true);

    const notCancelled = fireEvent.click(
      screen.getByRole("button", { name: "New standalone plan" }),
    );

    expect(notCancelled).toBe(false);
    expect(details().open).toBe(false);
  });

  it("lists every plan with no cap and no more button", () => {
    mount({ checklists: [], standalone: standalonePlans(60), moves: [] });

    expect(titles()).toHaveLength(60);
    expect(within(region()).queryByRole("button", { name: /more$/ })).toBeNull();
  });

  it("sorts by title and by state from the header", () => {
    mount();

    fireEvent.click(screen.getByRole("button", { name: "Plan" }));
    expect(titles()).toEqual(["Cache", "Cleanup", "Fix search", "Old"]);
    expect(screen.getByRole("columnheader", { name: "Plan" }).getAttribute("aria-sort")).toBe(
      "ascending",
    );
    fireEvent.click(screen.getByRole("button", { name: "Plan" }));
    expect(titles()).toEqual(["Old", "Fix search", "Cleanup", "Cache"]);

    fireEvent.click(screen.getByRole("button", { name: "State" }));
    expect(titles()).toEqual(["Cache", "Fix search", "Cleanup", "Old"]);
    expect(screen.getByRole("columnheader", { name: "Plan" }).getAttribute("aria-sort")).toBeNull();
  });

  it("filters by title or slug from ten plans, and not below", () => {
    mount();
    expect(screen.queryByRole("searchbox", { name: "Filter plans" })).toBeNull();
  });

  it("narrows a long list by title or slug", () => {
    mount({
      checklists: [],
      standalone: [
        ...standalonePlans(9),
        {
          slug: "zeta",
          path: "doc/plans/PLAN-zeta.md",
          title: "Speed up search",
          state: "done",
          last_change: null,
          pressure_date: null,
          has_summary: false,
        },
      ],
      moves: [],
    });
    const filter = screen.getByRole("searchbox", { name: "Filter plans" });

    fireEvent.change(filter, { target: { value: "ZETA" } });
    expect(titles()).toEqual(["Speed up search"]);
    fireEvent.change(filter, { target: { value: "plan 3" } });
    expect(titles()).toEqual(["Plan 3"]);
    fireEvent.change(filter, { target: { value: "nothing" } });
    expect(titles()).toEqual([]);
    expect(region().textContent).toContain("No plan matches “nothing”.");
  });

  it("selects the shown plans from the header box, which shows a partial selection", () => {
    mount();
    const all = screen.getByRole("checkbox", {
      name: "Select all shown plans",
    }) as HTMLInputElement;

    fireEvent.click(screen.getByRole("checkbox", { name: "Select Cache" }));
    expect(all.checked).toBe(false);
    expect(all.indeterminate).toBe(true);
    expect(screen.getByRole("group", { name: "Selected plans" }).textContent).toContain(
      "1 selected",
    );

    fireEvent.click(all);
    expect(all.checked).toBe(true);
    expect(all.indeterminate).toBe(false);
    expect(screen.getByRole("group", { name: "Selected plans" }).textContent).toContain(
      "4 selected",
    );

    fireEvent.click(all);
    expect(screen.queryByRole("group", { name: "Selected plans" })).toBeNull();
  });

  it("clears the selection from the action bar", () => {
    mount();
    fireEvent.click(screen.getByRole("checkbox", { name: "Select Old" }));

    fireEvent.click(screen.getByRole("button", { name: "Clear" }));

    expect((screen.getByRole("checkbox", { name: "Select Old" }) as HTMLInputElement).checked).toBe(
      false,
    );
    expect(screen.queryByRole("group", { name: "Selected plans" })).toBeNull();
  });

  it("opens the move dialog with the selected plans listed", () => {
    mount();
    fireEvent.click(screen.getByRole("checkbox", { name: "Select Cache" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Select Old" }));

    fireEvent.click(screen.getByRole("button", { name: "Move to a feature…" }));

    const dialog = screen.getByRole("dialog", { name: "Move into a feature" });
    expect(dialog.textContent).toContain("Moving 2 plans");
    expect(within(dialog).getByText("Cache")).toBeTruthy();
    expect(within(dialog).getByText("Old")).toBeTruthy();
    expect(within(dialog).queryByText("Fix search")).toBeNull();
  });

  it("returns focus to the move button when the dialog closes", async () => {
    mount();
    fireEvent.click(screen.getByRole("checkbox", { name: "Select Cache" }));
    const opener = screen.getByRole("button", { name: "Move to a feature…" });
    opener.focus();
    fireEvent.click(opener);

    fireEvent.click(screen.getByRole("button", { name: "Close" }));

    await waitFor(() => expect(document.activeElement).toBe(opener));
  });

  it("opens kickoff with the small-work route state", () => {
    mount();

    fireEvent.click(screen.getByRole("button", { name: "New standalone plan" }));

    expect(screen.getByText('Kickoff state: {"small":true}')).toBeTruthy();
  });

  it("keeps the empty shelf open with its explanation and action", () => {
    mount({ checklists: [], standalone: [], moves: [] });

    expect(details().open).toBe(true);
    expect(
      screen.getByText("Bugfixes and small changes that need no product spec live here."),
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "New standalone plan" })).toBeTruthy();
    expect(screen.queryByRole("table")).toBeNull();
  });

  it("lists misplaced documents and opens their move dialog with the path fixed", () => {
    mount({
      checklists: [],
      standalone: [],
      moves: [
        {
          id: "move-vision",
          kind: "move_into_feature",
          label: "Move vision",
          target: "doc/product/index.md",
        },
      ],
    });

    expect(screen.getByText("doc/product/index.md").closest("li")?.textContent).toContain(
      "Move doc/product/index.md into a feature",
    );
    expect(
      screen.queryByText("Bugfixes and small changes that need no product spec live here."),
    ).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Move plan" }));

    const dialog = screen.getByRole("dialog", { name: "Move into a feature" });
    expect(within(dialog).getByText("doc/product/index.md")).toBeTruthy();
    expect(dialog.textContent).not.toContain("Moving");
  });

  it("retains readable plans and explains blocked authoring actions", () => {
    mount(project, true);
    fireEvent.click(screen.getByRole("checkbox", { name: "Select Cache" }));

    expect(screen.getByRole("link", { name: "Fix search" })).toBeTruthy();
    const newPlan = screen.getByRole("button", { name: "New standalone plan" });
    const move = screen.getByRole("button", { name: "Move to a feature…" });
    // Each blocked control is described by the real reason, and no third line repeats it.
    for (const button of [newPlan, move]) {
      expect(button.getAttribute("aria-disabled")).toBe("true");
      const described = document.getElementById(button.getAttribute("aria-describedby") ?? "");
      expect(described?.textContent).toBe("Alex holds authoring");
    }
    expect(screen.getAllByText("Alex holds authoring")).toHaveLength(2);
    expect(screen.queryByText("Authoring is unavailable")).toBeNull();
    fireEvent.click(newPlan);
    fireEvent.click(move);
    expect(screen.queryByText('Kickoff state: {"small":true}')).toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("makes formatting controls in server titles visible", () => {
    mount({ ...project, standalone: [{ ...project.standalone[0]!, title: "Fix‮search" }] });

    expect(screen.getByRole("link", { name: "Fix⟨U+202E⟩search" })).toBeTruthy();
    expect(screen.getByRole("checkbox", { name: "Select Fix⟨U+202E⟩search" })).toBeTruthy();
  });

  it("renders inline title markup while preserving visible controls and text isolation", () => {
    mount({
      ...project,
      standalone: [
        {
          ...project.standalone[0]!,
          title: "Rename `hooks/` to `loom-hooks/` with *care* and **tests**‮",
        },
      ],
    });

    const name = "Rename hooks/ to loom-hooks/ with care and tests⟨U+202E⟩";
    const link = screen.getByRole("link", { name });
    expect(screen.getByRole("checkbox", { name: `Select ${name}` })).toBeTruthy();
    const code = within(link).getByText("hooks/");
    expect(code.tagName).toBe("CODE");
    expect(within(link).getByText("loom-hooks/").tagName).toBe("CODE");
    expect(within(link).getByText("care").tagName).toBe("EM");
    expect(within(link).getByText("tests").tagName).toBe("STRONG");
    expect(link.textContent).not.toContain("`");
  });
});
