import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Provider } from "jotai/react";
import { createStore } from "jotai/vanilla";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import { checklistViewSchema, serverInfoSchema } from "@/plan/api/schema";
import { FeatureSwitcher } from "@/plan/header/feature-switcher";
import { activeFeatureAtom, projectAtom, serverInfoAtom } from "@/plan/state/atoms";

afterEach(cleanup);

function show({
  activeFeature = "demo",
  project = "Example repository",
  features = ["demo", "Feature / one"],
}: {
  activeFeature?: string | null;
  project?: string | null;
  features?: string[];
} = {}) {
  const store = createStore();
  const checklist = checklistViewSchema.parse(checklistFixture.checklists[0]);
  store.set(activeFeatureAtom, activeFeature);
  store.set(projectAtom, {
    checklists: features.map((feature) => ({ ...checklist, feature })),
    standalone: [],
    moves: [],
  });
  store.set(
    serverInfoAtom,
    project === null ? null : { ...serverInfoSchema.parse(serverInfoFixture), project },
  );
  render(
    <Provider store={store}>
      <MemoryRouter>
        <div data-loom-plan>
          <FeatureSwitcher />
        </div>
      </MemoryRouter>
    </Provider>,
  );
}

it("shows the active feature instead of the repository name", () => {
  show();

  expect(screen.getByText("demo")).toBeTruthy();
  expect(screen.queryByRole("heading")).toBeNull();
  expect(
    screen
      .getByRole("button", { name: "Switch feature, current: demo" })
      .getAttribute("aria-expanded"),
  ).toBe("false");
});

it("shows the repository name when no feature is active", () => {
  show({ activeFeature: null });

  expect(
    screen.getByRole("button", { name: "Switch feature, current: Example repository" }),
  ).toBeTruthy();
});

it("uses Project before server info arrives when no feature is active", () => {
  show({ activeFeature: null, project: null });

  expect(screen.getByRole("button", { name: "Switch feature, current: Project" })).toBeTruthy();
});

it("opens the feature list and marks only the active feature", async () => {
  show();
  const trigger = screen.getByRole("button", { name: "Switch feature, current: demo" });

  fireEvent.click(trigger);

  const dialog = await screen.findByRole("dialog", { name: "Switch feature" });
  const active = within(dialog).getByRole("link", { name: "demo" });
  const other = within(dialog).getByRole("link", { name: "Feature / one" });
  expect(trigger.getAttribute("aria-haspopup")).toBe("dialog");
  expect(trigger.getAttribute("aria-expanded")).toBe("true");
  expect(active.getAttribute("aria-current")).toBe("true");
  expect(active.querySelector("svg")).not.toBeNull();
  expect(other.getAttribute("aria-current")).toBeNull();
  expect(other.querySelector("svg")).toBeNull();
});

it("links to encoded feature routes and project navigation", async () => {
  show();

  fireEvent.click(screen.getByRole("button", { name: "Switch feature, current: demo" }));

  const dialog = await screen.findByRole("dialog", { name: "Switch feature" });
  expect(within(dialog).getByRole("link", { name: "demo" }).getAttribute("href")).toBe("/f/demo");
  expect(within(dialog).getByRole("link", { name: "Feature / one" }).getAttribute("href")).toBe(
    "/f/Feature%20%2F%20one",
  );
  expect(within(dialog).getByRole("link", { name: "Home" }).getAttribute("href")).toBe("/");
  expect(within(dialog).getByRole("link", { name: "New work…" }).getAttribute("href")).toBe("/new");
  expect(within(dialog).getByRole("separator")).toBeTruthy();
});

it("moves focus through feature and project links with arrow keys", async () => {
  show();
  fireEvent.click(screen.getByRole("button", { name: "Switch feature, current: demo" }));
  const dialog = await screen.findByRole("dialog", { name: "Switch feature" });
  const active = within(dialog).getByRole("link", { name: "demo" });
  const other = within(dialog).getByRole("link", { name: "Feature / one" });
  const home = within(dialog).getByRole("link", { name: "Home" });
  const newWork = within(dialog).getByRole("link", { name: "New work…" });
  await waitFor(() => expect(document.activeElement).toBe(active));

  fireEvent.keyDown(active, { key: "ArrowDown" });
  expect(document.activeElement).toBe(other);
  fireEvent.keyDown(other, { key: "ArrowDown" });
  expect(document.activeElement).toBe(home);
  fireEvent.keyDown(home, { key: "ArrowDown" });
  expect(document.activeElement).toBe(newWork);
  fireEvent.keyDown(newWork, { key: "ArrowDown" });
  expect(document.activeElement).toBe(active);
  fireEvent.keyDown(active, { key: "ArrowUp" });
  expect(document.activeElement).toBe(newWork);
});

it("closes on Escape and returns focus to the title button", async () => {
  show();
  const trigger = screen.getByRole("button", { name: "Switch feature, current: demo" });
  trigger.focus();
  fireEvent.click(trigger);
  const dialog = await screen.findByRole("dialog", { name: "Switch feature" });

  fireEvent.keyDown(within(dialog).getByRole("link", { name: "demo" }), { key: "Escape" });

  await waitFor(() => {
    expect(screen.queryByRole("dialog", { name: "Switch feature" })).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });
  expect(trigger.getAttribute("aria-expanded")).toBe("false");
});

it.each(["demo", "Feature / one", "Home", "New work…"])(
  "closes after selecting %s",
  async (name) => {
    show();
    const trigger = screen.getByRole("button", { name: "Switch feature, current: demo" });
    fireEvent.click(trigger);
    const dialog = await screen.findByRole("dialog", { name: "Switch feature" });

    fireEvent.click(within(dialog).getByRole("link", { name }));

    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Switch feature" })).toBeNull(),
    );
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
  },
);

it("shows only Home and New work in an empty project", async () => {
  show({ activeFeature: null, features: [] });

  fireEvent.click(
    screen.getByRole("button", { name: "Switch feature, current: Example repository" }),
  );

  const dialog = await screen.findByRole("dialog", { name: "Switch feature" });
  expect(
    within(dialog)
      .getAllByRole("link")
      .map((link) => link.textContent),
  ).toEqual(["Home", "New work…"]);
  expect(within(dialog).queryByRole("separator")).toBeNull();
});

it("renders control characters visibly in server names", async () => {
  show({ activeFeature: "demo\u202e", features: ["demo\u202e"] });

  fireEvent.click(screen.getByRole("button", { name: "Switch feature, current: demo⟨U+202E⟩" }));

  const dialog = await screen.findByRole("dialog", { name: "Switch feature" });
  expect(within(dialog).getByRole("link", { name: "demo⟨U+202E⟩" })).toBeTruthy();
});
