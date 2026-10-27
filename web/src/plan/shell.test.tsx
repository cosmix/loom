import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { checklistViewSchema } from "@/plan/api/schema";
import { Dock } from "@/plan/dock";
import { PlanShell } from "@/plan/shell";
import { checklistAtom, connectionAtom, unauthorizedAtom } from "@/plan/state/atoms";

vi.mock("@/plan/header-extras", () => ({
  HeaderExtras: ({ compact }: { compact: boolean }) => (
    <span data-testid="extras" data-compact={String(compact)} />
  ),
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

function setWide(wide: boolean) {
  vi.spyOn(window, "matchMedia").mockImplementation(
    (query) =>
      ({
        matches: wide,
        media: query,
        addEventListener() {},
        removeEventListener() {},
      }) as unknown as MediaQueryList,
  );
}

beforeEach(() => {
  store = createStore();
  store.set(checklistAtom, checklistViewSchema.parse(checklistFixture));
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
  });

  it("toggles help with the question mark key", () => {
    setWide(true);
    renderShell();
    expect(screen.queryByText("Open or close Help")).toBeNull();
    fireEvent.keyDown(window, { key: "?" });
    expect(screen.getByText("Open or close Help")).toBeTruthy();
    fireEvent.keyDown(window, { key: "?" });
    expect(screen.queryByText("Open or close Help")).toBeNull();
  });

  it("returns focus to the previously focused control when help closes", async () => {
    setWide(true);
    renderShell(<button type="button">outlet button</button>);
    const origin = screen.getByRole("button", { name: "outlet button" });
    origin.focus();
    fireEvent.keyDown(origin, { key: "?" });
    const dialog = screen.getByRole("dialog", { name: "Help" });
    // Wide enough for the 220px key column, so "Open or close Help" does not wrap.
    expect(dialog.className).toContain("sm:max-w-xl");
    expect(dialog.className).not.toContain("sm:max-w-sm");
    expect(document.activeElement).not.toBe(origin);
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(origin));
  });

  it("ignores the question mark key while typing", () => {
    setWide(true);
    renderShell(<input aria-label="field" />);
    fireEvent.keyDown(screen.getByLabelText("field"), { key: "?" });
    expect(screen.queryByText("Open or close Help")).toBeNull();
  });

  it("renders the dock as nothing", () => {
    const { container } = render(<Dock />);
    expect(container.innerHTML).toBe("");
  });

  it("mounts a tooltip inside the shell", () => {
    setWide(true);
    renderShell(
      <Tooltip>
        <TooltipTrigger>hover me</TooltipTrigger>
        <TooltipContent>tip</TooltipContent>
      </Tooltip>,
    );
    expect(screen.getByText("hover me")).toBeTruthy();
  });

  it("collapses the tracker and moves presence and share into the overflow menu when narrow", () => {
    setWide(false);
    renderShell();
    expect(screen.getByText("Step 2 of 2 · Product spec")).toBeTruthy();
    expect(screen.getByTestId("extras").getAttribute("data-compact")).toBe("true");
    expect(screen.queryByRole("button", { name: /^share/i })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "More" }));
    const menu = screen.getByRole("dialog", { name: "More" });
    expect(within(menu).getByRole("button", { name: /^share/i })).toBeTruthy();
    expect(within(menu).getByRole("button", { name: /people here/i })).toBeTruthy();
  });

  it("shows presence and share in the header when wide", () => {
    setWide(true);
    renderShell();
    expect(screen.getByTestId("extras").getAttribute("data-compact")).toBe("false");
    expect(screen.getByRole("button", { name: /^share/i })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "More" })).toBeNull();
  });
});
