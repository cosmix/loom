import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { describe, expect, it } from "vitest";

import type { ProjectChecklistView } from "@/plan/api/schema";
import { harness, installStartChatLifecycle } from "@/plan/chat/start-chat-test-kit";
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
    },
    {
      slug: "cache",
      path: "doc/plans/IN_PROGRESS-PLAN-cache.md",
      title: "Cache",
      state: "running",
    },
    { slug: "cleanup", path: "doc/plans/DONE-PLAN-cleanup.md", title: "Cleanup", state: "done" },
    { slug: "old", path: "doc/plans/RETIRED-PLAN-old.md", title: "Old", state: "retired" },
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

describe("StandaloneShelf", () => {
  it("lists plan titles, slugs and lifecycle labels as file links without per-plan offers", () => {
    mount({
      ...project,
      standalone: [
        { ...project.standalone[0], path: "doc/plans/PLAN-100%-fix.md" },
        ...project.standalone.slice(1),
      ],
    });

    expect(screen.getByRole("region", { name: "Standalone plans" })).toBeTruthy();
    expect(
      screen.getByRole("link", { name: "Fix search fix-search written" }).getAttribute("href"),
    ).toBe("/files/plans/PLAN-100%25-fix.md");
    expect(screen.getByRole("link", { name: "Cache cache running" }).getAttribute("href")).toBe(
      "/files/plans/IN_PROGRESS-PLAN-cache.md",
    );
    expect(screen.getByRole("link", { name: "Cleanup cleanup done" })).toBeTruthy();
    expect(screen.getByRole("link", { name: "Old old retired" })).toBeTruthy();
    expect(screen.getAllByRole("button")).toHaveLength(2);
  });

  it("opens kickoff with the small-work route state", () => {
    mount();

    fireEvent.click(screen.getByRole("button", { name: "New standalone plan" }));

    expect(screen.getByText('Kickoff state: {"small":true}')).toBeTruthy();
  });

  it("keeps the empty shelf and both actions visible", () => {
    mount({ checklists: [], standalone: [], moves: [] });

    expect(
      screen.getByText("Bugfixes and small changes that need no product spec live here."),
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "New standalone plan" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Move into a feature…" })).toBeTruthy();
  });

  it("opens the general move dialog with a choice of standalone plans", () => {
    mount();

    fireEvent.click(screen.getByRole("button", { name: "Move into a feature…" }));

    expect(screen.getByRole("dialog", { name: "Move into a feature" })).toBeTruthy();
    expect(
      screen.getByRole("radio", { name: "Fix search doc/plans/PLAN-fix-search.md" }),
    ).toBeTruthy();
  });

  it("returns focus to the move button when the dialog closes", async () => {
    mount();
    const opener = screen.getByRole("button", { name: "Move into a feature…" });
    opener.focus();
    fireEvent.click(opener);

    fireEvent.click(screen.getByRole("button", { name: "Close" }));

    await waitFor(() => expect(document.activeElement).toBe(opener));
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
    fireEvent.click(screen.getAllByRole("button", { name: "Move into a feature…" })[0]);

    expect(screen.getByRole("dialog", { name: "Move into a feature" })).toBeTruthy();
    const dialog = screen.getByRole("dialog", { name: "Move into a feature" });
    expect(within(dialog).getByText("doc/product/index.md")).toBeTruthy();
    expect(screen.queryByRole("group", { name: "Standalone plan" })).toBeNull();
  });

  it("retains readable plans and explains blocked authoring actions", () => {
    mount(project, true);

    expect(screen.getByRole("link", { name: "Fix search fix-search written" })).toBeTruthy();
    const newPlan = screen.getByRole("button", { name: "New standalone plan" });
    expect(newPlan.getAttribute("aria-disabled")).toBe("true");
    expect(screen.getByText("Alex holds authoring")).toBeTruthy();
    fireEvent.click(newPlan);
    expect(screen.queryByText('Kickoff state: {"small":true}')).toBeNull();
  });

  it("makes formatting controls in server titles visible", () => {
    mount({ ...project, standalone: [{ ...project.standalone[0], title: "Fix\u202Esearch" }] });

    expect(screen.getByRole("link", { name: "Fix⟨U+202E⟩search fix-search written" })).toBeTruthy();
  });

  it("renders inline title markup while preserving visible controls and text isolation", () => {
    mount({
      ...project,
      standalone: [
        {
          ...project.standalone[0],
          title: "Rename `hooks/` to `loom-hooks/` with *care* and **tests**\u202E",
        },
      ],
    });

    const link = screen.getByRole("link", {
      name: "Rename hooks/ to loom-hooks/ with care and tests⟨U+202E⟩ fix-search written",
    });
    const code = within(link).getByText("hooks/");
    expect(code.tagName).toBe("CODE");
    expect(within(link).getByText("loom-hooks/").tagName).toBe("CODE");
    expect(within(link).getByText("care").tagName).toBe("EM");
    expect(within(link).getByText("tests").tagName).toBe("STRONG");
    expect(code.parentElement?.style.unicodeBidi).toBe("isolate");
    expect(code.parentElement?.style.direction).toBe("ltr");
    expect(link.textContent).not.toContain("`");
  });
});
