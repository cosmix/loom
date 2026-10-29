import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import planFixture from "@/plan/api/fixtures/files-doc-plan.json";
import hello from "@/plan/api/fixtures/hello.json";
import runFixture from "@/plan/api/fixtures/pressure-run.json";
import { serverInfoSchema } from "@/plan/api/schema";
import {
  docViewSchema,
  type DocView,
  type EntryKind,
  type PlanView,
} from "@/plan/api/schema-files";
import { pressureRunSchema } from "@/plan/api/schema-pressure";
import { pendingAttachmentsAtom } from "@/plan/attachments";
import { focusComposerAtom } from "@/plan/chat/composer-hooks";
import { panelCollapsedAtom } from "@/plan/chat/prefs";
import { pressureRunAtom } from "@/plan/chat/pressure/use-pressure";
import { chatSheetOpenAtom } from "@/plan/chat/sheet-state";
import { authoringAtom, serverInfoAtom } from "@/plan/state/atoms";

import { FileDocHeading } from "./doc-heading-view";

function makeDoc(patch: Partial<DocView> = {}, plan: Partial<PlanView> = {}): DocView {
  const base = docViewSchema.parse(planFixture);
  return { ...base, plan: { ...base.plan!, ...plan }, ...patch };
}

function authorStore(canAuthor = true) {
  const store = createStore();
  store.set(serverInfoAtom, serverInfoSchema.parse({ ...hello.server, read_only: [] }));
  store.set(authoringAtom, canAuthor ? { holder: null } : hello.authoring);
  return store;
}

function mount(
  doc = makeDoc(),
  store = authorStore(),
  address = "/files/plans/demo/PLAN-file-mode.md",
) {
  return {
    store,
    ...render(
      <Provider store={store}>
        <MemoryRouter initialEntries={[address]}>
          <FileDocHeading doc={doc} />
        </MemoryRouter>
      </Provider>,
    ),
  };
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date(2026, 9, 29, 0, 15));
  window.localStorage.clear();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("FileDocHeading", () => {
  it("renders the project, encoded feature link, group and current document breadcrumb", () => {
    mount(makeDoc({ feature: "demo & space" }));
    const nav = screen.getByRole("navigation", { name: "Breadcrumb" });

    expect(
      within(nav)
        .getAllByRole("listitem")
        .map((item) => item.textContent),
    ).toEqual(["Planner", "demo & space", "Plans", "File mode"]);
    expect(within(nav).getByRole("link").getAttribute("href")).toBe("/f/demo%20%26%20space");
    expect(within(nav).getByText("Planner").closest("a")).toBeNull();
    expect(
      within(nav).getByText("File mode").closest("[aria-current]")?.getAttribute("aria-current"),
    ).toBe("page");
  });

  it("omits the feature breadcrumb for a standalone document", () => {
    mount(makeDoc({ feature: null }));
    const nav = screen.getByRole("navigation", { name: "Breadcrumb" });

    expect(
      within(nav)
        .getAllByRole("listitem")
        .map((item) => item.textContent),
    ).toEqual(["Planner", "Plans", "File mode"]);
    expect(within(nav).queryByRole("link")).toBeNull();
  });

  it.each<[EntryKind, string]>([
    ["vision", "Product"],
    ["section", "Product"],
    ["note", "Product"],
    ["knowledge", "Knowledge"],
    ["brief", "Briefs"],
    ["metaplan", "Files"],
    ["feature", "Files"],
    ["metaplan_entry", "Files"],
    ["brief_folder", "Files"],
    ["knowledge_folder", "Files"],
    ["review", "Files"],
    ["other", "Files"],
  ])("groups %s documents under %s", (kind, group) => {
    mount(makeDoc({ kind, plan: null }));
    const nav = screen.getByRole("navigation", { name: "Breadcrumb" });

    expect(within(nav).getByText(group)).toBeTruthy();
  });

  it.each<[{ state: PlanView["state"] }, string]>([
    [{ state: "written" }, "Written"],
    [{ state: "running" }, "Running"],
    [{ state: "done" }, "Done"],
    [{ state: "retired" }, "Retired"],
  ])("labels the $state state chip", (plan, label) => {
    mount(makeDoc({}, plan));

    expect(screen.getByText(label)).toBeTruthy();
  });

  it("links implemented sections and strikes through a missing target without a link", () => {
    const { container } = mount();

    expect(screen.getByRole("link", { name: "Implements §6" }).getAttribute("href")).toBe(
      "/files/product/demo/section_06.md",
    );
    const missing = screen.getByText("Implements §7");
    const chip = missing.closest("[title]");
    expect(missing.closest("a")).toBeNull();
    expect(chip?.getAttribute("title")).toBe("not found");
    expect(chip?.textContent).toBe("Implements §7 (not found)");
    expect(chip?.querySelector(".sr-only")?.textContent).toBe("(not found)");
    expect(container.firstElementChild?.className).toContain(
      "[&_[title='not_found']]:line-through",
    );
  });

  it("shows validation errors with danger tone and toggles their list with pressed state", () => {
    mount();
    const toggle = screen.getByRole("button", { name: "1 error, 1 warning" });
    expect(toggle.getAttribute("aria-pressed")).toBe("false");
    expect(toggle.className).toContain("text-(--tone-blocked)");
    expect(screen.queryByTestId("validation-list")).toBeNull();

    fireEvent.click(toggle);

    expect(toggle.getAttribute("aria-pressed")).toBe("true");
    const list = screen.getByTestId("validation-list");
    expect(within(list).getByRole("link", { name: "build" }).getAttribute("href")).toBe(
      "#stage:build",
    );
    expect(within(list).getByText("Review dependencies")).toBeTruthy();

    fireEvent.click(toggle);
    expect(toggle.getAttribute("aria-pressed")).toBe("false");
    expect(screen.queryByTestId("validation-list")).toBeNull();
  });

  it("counts warnings across every bucket and labels an error-free result Valid", () => {
    const validation = makeDoc().plan!.validation!;
    mount(
      makeDoc(
        {},
        {
          validation: {
            ...validation,
            valid: true,
            errors: [],
            warnings: { structural: ["a"], knowledge: ["b"], sandbox: ["c"], baseline: ["d"] },
          },
        },
      ),
    );

    expect(screen.getByRole("button", { name: "Valid · 4 warnings" })).toBeTruthy();
  });

  it("labels zero warnings and omits validation when no result is available", () => {
    const validation = makeDoc().plan!.validation!;
    mount(
      makeDoc(
        {},
        {
          validation: {
            ...validation,
            valid: true,
            errors: [],
            warnings: { structural: [], knowledge: [], sandbox: [], baseline: [] },
          },
        },
      ),
    );
    expect(screen.getByRole("button", { name: "Valid · 0 warnings" })).toBeTruthy();
    cleanup();

    mount(makeDoc({}, { validation: null }));
    expect(screen.queryByRole("button", { name: /warnings?/ })).toBeNull();
  });

  it.each<[string | null, string]>([
    [null, "Not pressure-tested"],
    ["2026-10-17", "Pressure-tested 2026-10-17"],
    ["2026-10-29", "Pressure-tested today"],
  ])("labels the pressure date %s", (pressure_date, label) => {
    mount(makeDoc({}, { pressure_date }));

    expect(screen.getByText(label)).toBeTruthy();
  });

  it("prioritizes an active pressure run and hides Edit until the run ends", () => {
    const doc = makeDoc();
    const store = authorStore();
    const run = pressureRunSchema.parse({ ...runFixture, plan: doc.path });
    store.set(pressureRunAtom, run);
    mount(doc, store);
    expect(screen.getByText("Being pressure-tested")).toBeTruthy();
    expect(screen.queryByText("Pressure-tested 2026-10-17")).toBeNull();
    expect(screen.queryByRole("link", { name: "Edit" })).toBeNull();

    act(() =>
      store.set(pressureRunAtom, {
        ...run,
        ended: { kind: "recorded", by: null, step: null, reason: null, line: null },
      }),
    );

    expect(screen.queryByText("Being pressure-tested")).toBeNull();
    expect(screen.getByText("Pressure-tested 2026-10-17")).toBeTruthy();
    expect(screen.getByRole("link", { name: "Edit" })).toBeTruthy();
  });

  it("ignores an active pressure run belonging to another plan", () => {
    const store = authorStore();
    store.set(
      pressureRunAtom,
      pressureRunSchema.parse({ ...runFixture, plan: "doc/plans/other.md" }),
    );
    mount(makeDoc(), store);

    expect(screen.queryByText("Being pressure-tested")).toBeNull();
    expect(screen.getByRole("link", { name: "Edit" })).toBeTruthy();
  });

  it("shows relative change time with the exact timestamp and existing plan actions", () => {
    vi.setSystemTime(new Date("2026-10-29T22:51:53Z"));
    mount(makeDoc({}, { last_change: "2026-10-29T20:51:53Z" }));

    expect(screen.getByText("Changed 2 h ago").closest("[title]")?.getAttribute("title")).toBe(
      "2026-10-29T20:51:53Z",
    );
    expect(screen.getByRole("button", { name: "Pressure-test" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Start a run" })).toBeTruthy();
  });

  it("links a plan's Summary and marks its file Source as current", () => {
    mount(makeDoc({ feature: "demo" }, { slug: "file-mode" }));
    const views = screen.getByRole("group", { name: "View" });

    expect(within(views).getByRole("link", { name: "Summary" }).getAttribute("href")).toBe(
      "/f/demo/engineering/file-mode",
    );
    const source = within(views).getByRole("link", { name: "Source" });
    expect(source.getAttribute("href")).toBe("/files/plans/demo/PLAN-file-mode.md");
    expect(source.getAttribute("aria-current")).toBe("page");
  });

  it.each<[EntryKind, string | null, string | null]>([
    ["section", "demo", "/f/demo/product/section_03"],
    ["vision", "demo", "/f/demo/product"],
    ["metaplan", "demo", "/f/demo/engineering"],
    ["plan", null, "/plans/file-mode"],
    ["section", null, null],
    ["vision", null, null],
    ["metaplan", null, null],
    ["note", "demo", null],
  ])("links the Summary for kind=%s feature=%s", (kind, feature, target) => {
    mount(
      makeDoc(
        {
          kind,
          feature,
          path: "doc/product/demo/section_03.md",
          ...(kind === "plan" ? {} : { plan: null }),
        },
        { slug: "file-mode" },
      ),
    );
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

  it.each<[boolean, boolean, string, boolean]>([
    [true, true, "", true],
    [false, true, "", false],
    [true, false, "", false],
    [true, true, "?edit=1", false],
    [true, true, "?edit=0", true],
  ])("gates Edit for editable=%s author=%s query=%s", (editable, author, query, visible) => {
    mount(makeDoc({ editable }), authorStore(author), `/files/example.md${query}`);

    const edit = screen.queryByRole("link", { name: "Edit" });
    expect(edit !== null).toBe(visible);
    if (visible)
      expect(edit?.getAttribute("href")).toBe("/files/plans/demo/PLAN-file-mode.md?edit=1");
  });

  it("renders the lock notice and hides Edit for a locked, non-editable document", () => {
    mount(
      makeDoc({
        editable: false,
        lock: {
          reason: "running",
          plan: "file-mode",
          notice:
            "This plan is **running**. `loom run` uses the *committed* copy. [Help](https://example.com)",
        },
      }),
    );

    const note = screen.getByRole("note");
    expect(note.querySelector("code")?.textContent).toBe("loom run");
    expect(note.querySelector("strong")?.textContent).toBe("running");
    expect(note.querySelector("em")?.textContent).toBe("committed");
    expect(within(note).queryByRole("link")).toBeNull();
    expect(screen.queryByRole("link", { name: "Edit" })).toBeNull();
  });

  it("renders a non-plan document without plan chips, summary links or plan actions", () => {
    mount(
      makeDoc({
        feature: null,
        kind: "other",
        plan: null,
        editable: false,
        title: "Read me",
        path: "doc/read me.md",
      }),
      authorStore(false),
    );

    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("Read me");
    expect(screen.getByText("doc/read me.md")).toBeTruthy();
    expect(screen.getByRole("link", { name: "Uncommitted changes" }).getAttribute("href")).toBe(
      "/files/read%20me.md?diff=head",
    );
    expect(screen.queryByRole("link", { name: "Summary" })).toBeNull();
    expect(screen.getByText("Summary").getAttribute("aria-disabled")).toBe("true");
    expect(screen.queryByText("Written")).toBeNull();
    expect(screen.queryByText(/pressure-tested/)).toBeNull();
    expect(screen.queryByText(/^Changed /)).toBeNull();
    expect(screen.queryByRole("button", { name: "Pressure-test" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Discuss" })).toBeNull();
    expect(screen.queryByRole("note")).toBeNull();
  });

  it("Discuss attaches the whole document by path and opens and focuses chat", () => {
    const doc = makeDoc({ text: "📝".repeat(2_001) });
    const store = authorStore();
    store.set(panelCollapsedAtom, true);
    mount(doc, store);

    fireEvent.click(screen.getByRole("button", { name: "Discuss" }));

    expect(store.get(pendingAttachmentsAtom)).toEqual([
      {
        kind: "source",
        path: doc.path,
        anchor: null,
        lines: null,
        stageId: null,
        text: "📝".repeat(2_000),
        hash: doc.hash,
      },
    ]);
    expect(store.get(panelCollapsedAtom)).toBe(false);
    expect(store.get(chatSheetOpenAtom)).toBe(true);
    expect(store.get(focusComposerAtom)).toBe(true);
  });
});
