import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { Provider } from "jotai/react";
import { createStore } from "jotai/vanilla";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import { checklistViewSchema, serverInfoSchema } from "@/plan/api/schema";
import { PlanHeader } from "@/plan/header/header";
import { activeFeatureAtom, checklistAtom, serverInfoAtom } from "@/plan/state/atoms";

const NEXT_STEP = "Section 05 is the next product spec document to write.";
const OFFER_LABEL = checklistFixture.checklists[0]!.primary!.label;

vi.mock("@/plan/header-extras", () => ({
  HeaderExtras: ({ compact }: { compact: boolean }) =>
    compact ? <button type="button">Chat</button> : null,
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

it("shows the route feature's slug under the Planning eyebrow", () => {
  show("/f/demo");

  expect(screen.getByText("Planning · feature")).toBeTruthy();
  const switcher = screen.getByRole("button", { name: "Switch feature, current: demo" });
  expect(switcher.textContent).toBe("demo");
});

it.each([
  ["/", "Planning · project", "Example project"],
  ["/new", "Planning · new feature", "Untitled"],
  ["/files", "Planning · files", "Example project"],
  ["/plans/standalone", "Planning · files", "Example project"],
])("names %s with the eyebrow %s over %s", (pathname, eyebrow, title) => {
  show(pathname);

  expect(screen.getByText(eyebrow)).toBeTruthy();
  expect(screen.getByRole("button", { name: `${title}, switch feature` }).textContent).toBe(title);
});

it("keeps the route feature name before the server info arrives", () => {
  show("/f/demo", true, null);

  expect(screen.getByRole("button", { name: "Switch feature, current: demo" })).toBeTruthy();
});

it("includes the shared theme toggle", () => {
  show();

  expect(screen.getByRole("button", { name: /^Switch to/ })).toBeTruthy();
});

it("labels Settings and Help beside their icons", () => {
  show();

  expect(screen.getByRole("button", { name: "Settings" }).textContent).toBe("Settings");
  expect(screen.getByRole("button", { name: "Help" }).textContent).toBe("Help");
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

const NO_PROGRESS = '.plan-next-step, .plan-header-rail, [class*="plan-step"]';

it.each(["/f/demo", "/f/demo/product", "/f/demo/engineering/planning"])(
  "draws no step rail or next-step card on feature route %s",
  (pathname) => {
    const { container } = show(pathname);

    expect(screen.queryByRole("list", { name: "Steps" })).toBeNull();
    expect(screen.queryByTitle(NEXT_STEP)).toBeNull();
    expect(container.querySelector(NO_PROGRESS)).toBeNull();
  },
);

it.each(["/", "/new", "/files", "/plans/standalone"])(
  "omits feature progress on %s after a feature was selected",
  (pathname) => {
    const { store } = show(pathname);

    expect(store.get(activeFeatureAtom)).toBe("demo");
    expect(screen.getByRole("button", { name: /, switch feature$/ })).toBeTruthy();
    expect(screen.queryByRole("list", { name: "Steps" })).toBeNull();
    expect(screen.queryByTitle(NEXT_STEP)).toBeNull();
  },
);

it.each(["/f/other", "/f/other/product", "/f/other/engineering/planning"])(
  "names the route feature on %s while the active feature is stale",
  (pathname) => {
    const { store } = show(pathname);

    expect(store.get(activeFeatureAtom)).toBe("demo");
    expect(screen.getByRole("button", { name: "Switch feature, current: other" })).toBeTruthy();
    expect(screen.queryByRole("list", { name: "Steps" })).toBeNull();
  },
);

it("keeps no feature progress when navigating from a feature to Files", () => {
  const { store } = show("/f/demo");

  fireEvent.click(screen.getByRole("link", { name: "Files" }));

  expect(store.get(activeFeatureAtom)).toBe("demo");
  expect(screen.queryByRole("list", { name: "Steps" })).toBeNull();
  expect(screen.queryByTitle(NEXT_STEP)).toBeNull();
});

it("uses the overflow control below 1024px and draws no rail or card", () => {
  const { container } = show("/f/demo", false);

  expect(screen.getByRole("button", { name: "More" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: /Step \d of 6/ })).toBeNull();
  expect(screen.queryByText(OFFER_LABEL)).toBeNull();
  expect(container.querySelector(NO_PROGRESS)).toBeNull();
});

it("keeps one row of logo, identity, activity and overflow on phones", () => {
  const { container } = show("/f/demo", false);
  const controls = container.querySelector<HTMLDivElement>(".plan-header-controls")!;

  expect(controls.classList.contains("header-controls")).toBe(true);
  expect(controls.parentElement!.classList.contains("plan-appbar")).toBe(true);
  expect(within(controls).getByRole("button", { name: "More" })).toBeTruthy();
  expect(within(controls).getByRole("button", { name: "Chat" })).toBeTruthy();
  // Everything else sits behind the overflow menu.
  for (const name of ["Settings", "Help", /^Switch to/]) {
    expect(within(controls).queryByRole("button", { name })).toBeNull();
  }
  expect(within(controls).queryByRole("link", { name: "Files" })).toBeNull();
});

it("shows no chat button where the dock mounts no panel", () => {
  show("/", false);

  expect(screen.queryByRole("button", { name: "Chat" })).toBeNull();
});

it.each([true, false])("keeps the app bar to one row at wide=%s", (wide) => {
  const { container } = show("/f/demo", wide);

  const bar = container.querySelector(".plan-appbar")!;
  expect(container.querySelectorAll(".plan-appbar")).toHaveLength(1);
  expect(bar.querySelector(NO_PROGRESS)).toBeNull();
  // Logo, identity and the controls cluster are the row's only three cells.
  expect([...bar.children].map((cell) => cell.className)).toEqual([
    "plan-logo-link",
    "plan-identity",
    "plan-header-controls header-controls",
  ]);
});

it("lists no operation as in progress when idle", () => {
  show();

  expect(screen.queryByRole("button", { name: /in progress/i })).toBeNull();
  expect(screen.queryByText(/in progress/i)).toBeNull();
});
