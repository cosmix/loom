import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Provider } from "jotai/react";
import { createStore } from "jotai/vanilla";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import { checklistViewSchema, serverInfoSchema } from "@/plan/api/schema";
import { PlanHeader } from "@/plan/header/header";
import { checklistAtom, serverInfoAtom } from "@/plan/state/atoms";

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
  store.set(checklistAtom, checklistViewSchema.parse(checklistFixture));
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
  return { ...result, onOpenSettings, onOpenHelp };
}

it("links the Loom logo to the planner home", () => {
  show();

  const link = screen.getByRole("link", { name: "Loom planner home" });
  expect(link.getAttribute("href")).toBe("/");
  expect(link.querySelector("svg")).not.toBeNull();
});

it("shows the project name under the Planning eyebrow", () => {
  show();

  expect(screen.getByText("Planning")).toBeTruthy();
  expect(screen.getByRole("heading", { name: "Example project" })).toBeTruthy();
});

it("uses Project before the server info arrives", () => {
  show("/files", true, null);

  expect(screen.getByRole("heading", { name: "Project" })).toBeTruthy();
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

it("shows the step rail and next-step card on the files view", () => {
  show("/files");

  expect(screen.getByRole("list", { name: "Steps" })).toBeTruthy();
  expect(screen.getByTitle("Write section 05")).toBeTruthy();
});

it("omits the step rail and next-step card on the project home", () => {
  show("/");

  expect(screen.queryByRole("list", { name: "Steps" })).toBeNull();
  expect(screen.queryByTitle("Write section 05")).toBeNull();
});

it("collapses the step rail and uses the overflow control below 1024px", () => {
  show("/files", false);

  expect(screen.getByRole("button", { name: /Step 2 of 2 · Product spec/ })).toBeTruthy();
  expect(screen.getByRole("button", { name: "More" })).toBeTruthy();
  expect(screen.queryByRole("list", { name: "Steps" })).toBeNull();
  expect(screen.getByTitle("Write section 05")).toBeTruthy();
});

it("omits the activity pill when idle", () => {
  show();

  expect(screen.queryByRole("button", { name: /in progress/i })).toBeNull();
  expect(screen.queryByText(/in progress/i)).toBeNull();
});
