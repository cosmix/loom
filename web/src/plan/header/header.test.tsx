import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { Provider } from "jotai/react";
import { createStore } from "jotai/vanilla";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import { checklistViewSchema, serverInfoSchema } from "@/plan/api/schema";
import { PlanHeader } from "@/plan/header/header";
import { activeFeatureAtom, checklistAtom, projectAtom, serverInfoAtom } from "@/plan/state/atoms";

const NEXT_STEP = "Section 05 is the next product spec document to write.";

vi.mock("@/plan/header-extras", () => ({
  HeaderExtras: () => <button type="button">Chat</button>,
}));
vi.mock("@/plan/header/name-prompt", () => ({ useResendStoredName: () => {} }));
vi.mock("@/plan/header/overflow", () => ({ Overflow: () => <button type="button">More</button> }));
vi.mock("@/plan/header/presence", () => ({ Presence: () => null }));
vi.mock("@/plan/header/share", () => ({ Share: () => null }));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function show(pathname = "/files", wide = true, project: string | null = "Example project") {
  vi.spyOn(window, "matchMedia").mockImplementation((query) => ({
    matches: query === "(min-width: 1024px)" && wide,
    media: query,
    onchange: null,
    addEventListener() {},
    removeEventListener() {},
    addListener() {},
    removeListener() {},
    dispatchEvent: () => false,
  }));
  const store = createStore();
  store.set(checklistAtom, checklistViewSchema.parse(checklistFixture.checklists[0]));
  store.set(
    serverInfoAtom,
    project === null
      ? null
      : {
          ...serverInfoSchema.parse(serverInfoFixture),
          project,
        },
  );
  const onOpenSettings = vi.fn();
  const onOpenHelp = vi.fn();
  const result = render(
    <Provider store={store}>
      <MemoryRouter initialEntries={[pathname]}>
        <div data-loom-plan>
          <PlanHeader onOpenSettings={onOpenSettings} onOpenHelp={onOpenHelp} />
        </div>
      </MemoryRouter>
    </Provider>,
  );
  return { ...result, store, onOpenSettings, onOpenHelp };
}

it("links the Loom logo to the planner home", () => {
  show();

  const link = screen.getByRole("link", { name: "Loom planner home" });
  expect(link.getAttribute("href")).toBe("/");
  expect(link.querySelector("svg")).not.toBeNull();
});

it("shows the route feature under the Planning eyebrow", () => {
  show("/f/demo");

  expect(screen.getByText("Planning")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Switch feature, current: demo" })).toBeTruthy();
});

it("keeps the route feature name before the server info arrives", () => {
  show("/f/demo", true, null);

  expect(screen.getByRole("button", { name: "Switch feature, current: demo" })).toBeTruthy();
});

it("includes the shared theme toggle", () => {
  show();

  expect(screen.getByRole("button", { name: /^Switch to/ })).toBeTruthy();
});

it("opens settings with its callback", () => {
  const { onOpenSettings, onOpenHelp } = show();

  fireEvent.click(screen.getByRole("button", { name: "Settings" }));

  expect(onOpenSettings).toHaveBeenCalledOnce();
  expect(onOpenHelp).not.toHaveBeenCalled();
});

it("opens help with its callback", () => {
  const { onOpenSettings, onOpenHelp } = show();

  fireEvent.click(screen.getByRole("button", { name: "Help" }));

  expect(onOpenHelp).toHaveBeenCalledOnce();
  expect(onOpenSettings).not.toHaveBeenCalled();
});

it.each(["/f/demo", "/f/demo/product", "/f/demo/engineering/planning"])(
  "shows the step rail and next-step card on feature route %s",
  (pathname) => {
    show(pathname);

    expect(screen.getByRole("list", { name: "Steps" })).toBeTruthy();
    expect(screen.getByTitle(NEXT_STEP)).toBeTruthy();
  },
);

it.each(["/", "/new", "/files", "/plans/standalone"])(
  "omits feature progress on %s after a feature was selected",
  (pathname) => {
    const { store } = show(pathname);

    expect(store.get(activeFeatureAtom)).toBe("demo");
    expect(screen.getByRole("button", { name: "Example project, switch feature" })).toBeTruthy();
    expect(screen.queryByRole("list", { name: "Steps" })).toBeNull();
    expect(screen.queryByTitle(NEXT_STEP)).toBeNull();
  },
);

it.each(["/f/other", "/f/other/product", "/f/other/engineering/planning"])(
  "uses the route feature's checklist on %s while the active feature is stale",
  (pathname) => {
    const { store } = show(pathname);
    const demo = checklistViewSchema.parse(checklistFixture.checklists[0]);
    act(() => {
      store.set(projectAtom, {
        checklists: [
          demo,
          {
            ...demo,
            feature: "other",
            steps: [{ number: 1, name: "Other vision", mark: "done" }],
            current: { kind: "ready_all_done" },
            primary: null,
            items: [],
          },
        ],
        standalone: [],
        moves: [],
      });
    });

    expect(store.get(activeFeatureAtom)).toBe("demo");
    expect(screen.getByRole("button", { name: "Switch feature, current: other" })).toBeTruthy();
    const rail = screen.getByRole("list", { name: "Steps" });
    expect(within(rail).getByText("Other vision")).toBeTruthy();
    expect(within(rail).queryByText("Product spec")).toBeNull();
    expect(screen.getByTitle("All plans done")).toBeTruthy();
    expect(screen.queryByTitle(NEXT_STEP)).toBeNull();
  },
);

it("removes feature progress when navigating from a feature to Files", () => {
  const { store } = show("/f/demo");
  expect(screen.getByRole("list", { name: "Steps" })).toBeTruthy();

  fireEvent.click(screen.getByRole("link", { name: "Files" }));

  expect(store.get(activeFeatureAtom)).toBe("demo");
  expect(screen.queryByRole("list", { name: "Steps" })).toBeNull();
  expect(screen.queryByTitle(NEXT_STEP)).toBeNull();
});

it("collapses the step rail and uses the overflow control below 1024px", () => {
  show("/f/demo", false);

  expect(screen.getByRole("button", { name: /Step 2 of 2 · Product spec/ })).toBeTruthy();
  expect(screen.getByRole("button", { name: "More" })).toBeTruthy();
  expect(screen.queryByRole("list", { name: "Steps" })).toBeNull();
  expect(screen.getByTitle(NEXT_STEP)).toBeTruthy();
});

it("wraps the controls across the full header width on phones", () => {
  const { container } = show("/f/demo", false);
  const controls = container.querySelector<HTMLDivElement>(".plan-header-controls")!;

  expect(controls.classList.contains("flex-wrap")).toBe(true);
  expect(controls.classList.contains("col-span-2")).toBe(true);
  expect(controls.classList.contains("sm:col-span-1")).toBe(true);
  expect(controls.classList.contains("overflow-x-auto")).toBe(false);
  expect(controls.parentElement!.classList.contains("grid-cols-[auto_minmax(8rem,1fr)]")).toBe(
    true,
  );
  expect(within(controls).getByRole("link", { name: "Files" })).toBeTruthy();
  expect(within(controls).getByRole("button", { name: "More" })).toBeTruthy();
  expect(within(controls).getByRole("button", { name: /^Switch to/ })).toBeTruthy();
  expect(within(controls).getByRole("button", { name: "Settings" })).toBeTruthy();
  expect(within(controls).getByRole("button", { name: "Help" })).toBeTruthy();
});

it("reserves space for the collapsed rail beside the next-step card", () => {
  show("/f/demo", false);

  const trigger = screen.getByRole("button", { name: "Step 2 of 2 · Product spec" });
  const row = trigger.parentElement!.parentElement!;
  expect(row.classList.contains("grid-cols-[auto_minmax(0,1fr)]")).toBe(true);
  expect(row.classList.contains("lg:flex")).toBe(true);
  expect(within(trigger).getByText("Step 2 of 2 · Product spec")).toBeTruthy();
});

it("omits the activity pill when idle", () => {
  show();

  expect(screen.queryByRole("button", { name: /in progress/i })).toBeNull();
  expect(screen.queryByText(/in progress/i)).toBeNull();
});
