import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { checklistViewSchema } from "@/plan/api/schema";
import { Dock, hasChatPanel } from "@/plan/dock";
import { readingFaceAtom, readingSizeAtom } from "@/plan/settings/preferences";
import { PlanShell } from "@/plan/shell";
import { checklistAtom, connectionAtom, unauthorizedAtom } from "@/plan/state/atoms";

vi.mock("@/plan/header/header", () => ({
  PlanHeader: ({
    onOpenSettings,
    onOpenHelp,
  }: {
    onOpenSettings: () => void;
    onOpenHelp: () => void;
  }) => (
    <>
      <button type="button" onClick={onOpenSettings}>
        Settings
      </button>
      <button type="button" onClick={onOpenHelp}>
        Help
      </button>
    </>
  ),
}));

vi.mock("@/plan/help/help-dialog", () => ({
  PlannerHelpDialog: ({ open }: { open: boolean }) =>
    open ? <div data-testid="help-dialog" /> : null,
}));

vi.mock("@/plan/settings/settings-dialog", () => ({
  SettingsDialog: ({ open }: { open: boolean }) =>
    open ? <div data-testid="settings-dialog" /> : null,
}));

let store: ReturnType<typeof createStore>;

function renderShell(outlet = <p>outlet content</p>) {
  return render(
    <Provider store={store}>
      <MemoryRouter initialEntries={["/"]}>
        <Routes>
          <Route path="/" element={<PlanShell />}>
            <Route index element={outlet} />
          </Route>
        </Routes>
      </MemoryRouter>
    </Provider>,
  );
}

beforeEach(() => {
  store = createStore();
  store.set(checklistAtom, checklistViewSchema.parse(checklistFixture.checklists[0]));
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("PlanShell", () => {
  it("leads the unauthorized screen with the restart sentence after a connection", () => {
    store.set(unauthorizedAtom, true);
    store.set(connectionAtom, { phase: "reconnecting", lastMessageAt: null, everOpen: true });
    const { container } = renderShell();
    const text = (container.textContent ?? "").replace(/\s+/g, " ");
    expect(text).toContain("loom plan restarted. Open the URL it printed in the terminal.");
    expect(text).toContain("Open the link loom plan printed.");
    expect(text).toContain("Running loom plan again in this project prints it.");
    expect(screen.queryByText("outlet content")).toBeNull();
    expect(screen.queryByRole("button", { name: "Settings" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Help" })).toBeNull();
    fireEvent.keyDown(window, { key: "?" });
    expect(screen.queryByTestId("help-dialog")).toBeNull();
    expect(screen.queryByTestId("settings-dialog")).toBeNull();
  });

  it("toggles help with the question mark key", async () => {
    renderShell();
    expect(screen.queryByTestId("help-dialog")).toBeNull();
    fireEvent.keyDown(window, { key: "?" });
    // The help dialog is a lazy chunk: the first open resolves after a tick.
    await screen.findByTestId("help-dialog");
    expect(screen.getByTestId("help-dialog")).toBeTruthy();
    fireEvent.keyDown(window, { key: "?" });
    expect(screen.queryByTestId("help-dialog")).toBeNull();
  });

  it("opens help from the header", async () => {
    renderShell();
    expect(screen.queryByTestId("help-dialog")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Help" }));
    await screen.findByTestId("help-dialog");
    expect(screen.getByTestId("help-dialog")).toBeTruthy();
    expect(screen.queryByTestId("settings-dialog")).toBeNull();
  });

  it("opens settings from the header", () => {
    renderShell();
    expect(screen.queryByTestId("settings-dialog")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(screen.getByTestId("settings-dialog")).toBeTruthy();
    expect(screen.queryByTestId("help-dialog")).toBeNull();
  });

  it("ignores the question mark key while typing", () => {
    renderShell(<input aria-label="field" />);
    fireEvent.keyDown(screen.getByLabelText("field"), { key: "?" });
    expect(screen.queryByTestId("help-dialog")).toBeNull();
  });

  it.each(["altKey", "ctrlKey", "metaKey"])("ignores question mark with %s", (modifier) => {
    renderShell();
    fireEvent.keyDown(window, { key: "?", [modifier]: true });
    expect(screen.queryByTestId("help-dialog")).toBeNull();
  });

  it("sets the default reading attributes on the planner root", () => {
    const { container } = renderShell();
    const root = container.querySelector("[data-loom-plan]");
    expect(root?.getAttribute("data-reading-face")).toBe("source-serif");
    expect(root?.getAttribute("data-reading-size")).toBe("19");
  });

  it("updates the reading face attribute when the preference changes", () => {
    const { container } = renderShell();
    const root = container.querySelector("[data-loom-plan]");
    act(() => store.set(readingFaceAtom, "literata"));
    expect(root?.getAttribute("data-reading-face")).toBe("literata");
  });

  it("updates the reading size attribute when the preference changes", () => {
    const { container } = renderShell();
    const root = container.querySelector("[data-loom-plan]");
    act(() => store.set(readingSizeAtom, 21));
    expect(root?.getAttribute("data-reading-size")).toBe("21");
  });

  it("renders the dock as nothing", () => {
    const { container } = render(
      <MemoryRouter>
        <Dock />
      </MemoryRouter>,
    );
    expect(container.innerHTML).toBe("");
  });

  it("keeps the chat panel off the home screen and the kickoff only", () => {
    expect(["/", "/new"].map(hasChatPanel)).toEqual([false, false]);
    expect(
      ["/f/demo", "/f/demo/product", "/files", "/plans/x", "/changes"].map(hasChatPanel),
    ).toEqual([true, true, true, true, true]);
  });

  it("mounts a tooltip inside the shell", () => {
    renderShell(
      <Tooltip>
        <TooltipTrigger>hover me</TooltipTrigger>
        <TooltipContent>tip</TooltipContent>
      </Tooltip>,
    );
    expect(screen.getByText("hover me")).toBeTruthy();
  });
});
