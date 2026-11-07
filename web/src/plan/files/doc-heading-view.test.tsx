import { act, cleanup, fireEvent, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import runFixture from "@/plan/api/fixtures/pressure-run.json";
import { sessionViewSchema } from "@/plan/api/schema-chat";
import type { EntryKind, PlanView } from "@/plan/api/schema-files";
import { pressureRunSchema } from "@/plan/api/schema-pressure";
import { pendingAttachmentsAtom } from "@/plan/attachments";
import { focusComposerAtom } from "@/plan/chat/composer-hooks";
import { snapshotAtom } from "@/plan/chat/model";
import { offerErrorAtom } from "@/plan/chat/offer-error";
import { pressureRunAtom } from "@/plan/chat/pressure/use-pressure";
import { screenAtom } from "@/plan/chat/session-home";
import { chatSheetOpenAtom } from "@/plan/chat/sheet-state";

import { authorStore, makeDoc, mount, withFeature } from "./doc-heading-kit";
import { pressureWhen } from "./heading-parts";
import { registerDocHeadingHooks } from "./doc-heading-view.fixtures";

registerDocHeadingHooks();

describe("FileDocHeading", () => {
  it("renders the feature slug, group and plan slug as the breadcrumb", () => {
    mount(makeDoc({ feature: "demo & space" }), withFeature(authorStore(), "demo & space"));
    const nav = screen.getByRole("navigation", { name: "Breadcrumb" });

    expect(
      within(nav)
        .getAllByRole("listitem")
        .map((item) => item.textContent),
    ).toEqual(["demo & space", "Plans", "file-mode"]);
    const first = within(nav).getByRole("link", { name: "demo & space" });
    expect(first.getAttribute("href")).toBe("/f/demo%20%26%20space");
    expect(first.querySelector("svg.lucide-layers")).not.toBeNull();
    expect(within(nav).queryByText("Planner")).toBeNull();
    const slug = within(nav).getByText("file-mode").closest("[aria-current]");
    expect(slug?.getAttribute("aria-current")).toBe("page");
    expect(slug?.className).toContain("font-mono");
  });

  it("starts a standalone document's breadcrumb at its group", () => {
    mount(makeDoc({ feature: null }));
    const nav = screen.getByRole("navigation", { name: "Breadcrumb" });

    expect(
      within(nav)
        .getAllByRole("listitem")
        .map((item) => item.textContent),
    ).toEqual(["Plans", "file-mode"]);
    expect(within(nav).queryByRole("link")).toBeNull();
  });

  it.each<[EntryKind, string]>([
    ["vision", "Product spec"],
    ["section", "Product spec"],
    ["note", "Product spec"],
    ["knowledge", "Knowledge"],
    ["brief", "Briefs"],
    ["metaplan", "Engineering"],
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

  it("links implemented sections in one chip and leaves a missing target plain", () => {
    const { container } = mount();

    const chip = screen.getByText("Implements", { exact: false }).closest("[data-chip]");
    expect(chip?.textContent).toBe("Implements §06, §07 (not found)");
    expect(screen.getByRole("link", { name: "§06" }).getAttribute("href")).toBe(
      "/files/product/demo/section_06.md",
    );
    const missing = screen.getByText("§07", { exact: false });
    expect(missing.closest("a")).toBeNull();
    expect(missing.closest("[title]")?.getAttribute("title")).toBe("not found");
    expect(missing.closest("[title]")?.querySelector(".sr-only")?.textContent).toBe("(not found)");
    expect(container.innerHTML).not.toContain("line-through");
  });

  it("shows validation errors with danger tone and toggles their list with pressed state", () => {
    mount();
    const toggle = screen.getByRole("button", { name: "1 error, 1 warning" });
    expect(toggle.getAttribute("aria-pressed")).toBe("false");
    expect(toggle.getAttribute("data-tone")).toBe("danger");
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

    const chip = screen.getByRole("button", { name: "Valid · 4 warnings" });
    expect(chip.getAttribute("data-tone")).toBe("warning");
    expect(chip.querySelector("svg.lucide-shield-check")).not.toBeNull();
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
    expect(
      screen.getByRole("button", { name: "Valid · 0 warnings" }).getAttribute("data-tone"),
    ).toBe("ok");
    cleanup();

    mount(makeDoc({}, { validation: null }));
    expect(screen.queryByRole("button", { name: /warnings?/ })).toBeNull();
  });

  it.each<[string | null, string]>([
    [null, "Not pressure-tested"],
    ["2026-10-17", `Pressure-tested ${pressureWhen("2026-10-17")}`],
    ["2026-10-29", "Pressure-tested today"],
  ])("labels the pressure date %s", (pressure_date, label) => {
    mount(makeDoc({}, { pressure_date }));

    const chip = screen.getByText(label).closest("[data-chip]");
    expect(chip?.getAttribute("data-tone")).toBe(pressure_date === null ? "warning" : "ok");
    expect(
      chip?.querySelector(pressure_date === null ? "svg.lucide-gauge" : "svg.lucide-flask-conical"),
    ).not.toBeNull();
  });

  it("sets Pressure-test at the end of the pressure status row, right after the plan's state", () => {
    mount(makeDoc({}, { pressure_date: null }));
    const row = screen.getByText("Not pressure-tested").closest("li");
    const button = screen.getByRole("button", { name: "Pressure-test" });
    expect(row?.contains(button)).toBe(true);
    expect(button.closest(".doc-heading-row__action")).not.toBeNull();
    // A button at rest: the flask and the class that fills it inside the card.
    expect(button.querySelector("svg.lucide-flask-conical")).not.toBeNull();
    expect(button.closest(".pressure-test-action")).not.toBeNull();
    expect(row?.previousElementSibling?.textContent).toContain("Written");
    cleanup();
    mount(makeDoc({}, { pressure_date: "2026-10-17" }));
    const tested = screen.getByText(`Pressure-tested ${pressureWhen("2026-10-17")}`);
    expect(tested.closest("[title]")?.getAttribute("title")).toBe("2026-10-17");
    expect(
      tested.closest("li")?.contains(screen.getByRole("button", { name: "Pressure-test again" })),
    ).toBe(true);
  });

  it("draws no Pressure-test for a plan that is not written", () => {
    mount(makeDoc({}, { state: "running" }));
    expect(screen.queryByRole("button", { name: /^Pressure-test/ })).toBeNull();
  });

  it("prioritizes an active pressure run and hides Edit until the run ends", () => {
    const doc = makeDoc();
    const store = authorStore();
    const run = pressureRunSchema.parse({ ...runFixture, plan: doc.path });
    store.set(pressureRunAtom, run);
    mount(doc, store);
    const running = screen.getByText("Being pressure-tested").closest("[data-chip]");
    // "Being pressure-tested" is a state word: the rotating arrows lead it.
    expect(running?.querySelector("svg.plan-arrow-spinner")).not.toBeNull();
    expect(screen.queryByText(`Pressure-tested ${pressureWhen("2026-10-17")}`)).toBeNull();
    expect(screen.queryByRole("link", { name: "Edit" })).toBeNull();

    act(() =>
      store.set(pressureRunAtom, {
        ...run,
        ended: { kind: "recorded", by: null, step: null, reason: null, line: null },
      }),
    );

    expect(screen.queryByText("Being pressure-tested")).toBeNull();
    expect(screen.getByText(`Pressure-tested ${pressureWhen("2026-10-17")}`)).toBeTruthy();
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

    const changed = screen.getByText("Changed 2 h ago").closest("[title]");
    expect(changed?.getAttribute("title")).toBe("2026-10-29T20:51:53Z");
    expect(changed?.querySelector("svg.lucide-clock")).not.toBeNull();
    expect(screen.getByRole("button", { name: "Pressure-test again" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Start a run" })).toBeNull();
  });

  it("sets Edit and Discuss in a toolbar group, on the view switch's centre line", () => {
    mount();
    const group = screen.getByRole("link", { name: "Edit" }).parentElement;
    expect(group?.className).toBe("doc-heading-group");
    expect(group?.contains(screen.getByRole("button", { name: "Discuss" }))).toBe(true);
    expect(group?.parentElement).toBe(
      screen.getByRole("group", { name: "View" }).parentElement?.parentElement,
    );
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

  it("renders the lock notice and shows Edit disabled with a lock for a locked document", () => {
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
    const edit = screen.getByRole("button", { name: "Edit" });
    expect(edit.getAttribute("aria-disabled")).toBe("true");
    expect(edit.getAttribute("title")).toBe("Read-only while running");
    expect(edit.querySelector("svg.lucide-lock")).not.toBeNull();
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
    mount(doc, store);

    fireEvent.click(screen.getByRole("button", { name: "Discuss" }));

    expect(store.get(pendingAttachmentsAtom)).toEqual([
      {
        kind: "source",
        path: doc.path,
        anchor: null,
        lines: null,
        stageId: null,
        text: `${"📝".repeat(2_000)}\n[…1 more characters; read the source]`,
        hash: doc.hash,
      },
    ]);
    expect(store.get(chatSheetOpenAtom)).toBe(true);
    expect(store.get(focusComposerAtom)).toBe(true);
  });

  it("Discuss refuses and adds nothing while an open chat belongs elsewhere", () => {
    const store = authorStore();
    const session = sessionViewSchema.parse({
      ...snapshotFixture.session,
      state: "idle",
      feature: "alpha",
    });
    store.set(snapshotAtom, { session, turns: [], queue: [], previous: null });
    store.set(screenAtom, { feature: "beta", plan: null });
    mount(makeDoc(), store);

    fireEvent.click(screen.getByRole("button", { name: "Discuss" }));

    expect(store.get(offerErrorAtom)).toBe("Finish the session in alpha first.");
    expect(store.get(pendingAttachmentsAtom)).toEqual([]);
    expect(store.get(focusComposerAtom)).toBe(false);
    expect(store.get(chatSheetOpenAtom)).toBe(true);
    // The same click attaches once the chat is on this screen, so the refusal above is the guard.
    store.set(screenAtom, { feature: "alpha", plan: null });
    fireEvent.click(screen.getByRole("button", { name: "Discuss" }));
    expect(store.get(pendingAttachmentsAtom)).toHaveLength(1);
    expect(store.get(focusComposerAtom)).toBe(true);
  });
});
