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

/** A list link by its target. */
function linkTo(dialog: HTMLElement, href: string): HTMLElement {
  const link = within(dialog)
    .getAllByRole("link")
    .find((candidate) => candidate.getAttribute("href") === href);
  if (!link) throw new Error(`no link to ${href}`);
  return link;
}

function show({
  activeFeature = "demo",
  project = "Example repository",
  features = ["demo", "Feature / one"],
  pathname = activeFeature === null ? "/" : `/f/${encodeURIComponent(activeFeature)}`,
}: {
  activeFeature?: string | null;
  project?: string | null;
  features?: string[];
  pathname?: string;
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
      <MemoryRouter initialEntries={[pathname]}>
        <div data-loom-plan>
          <FeatureSwitcher />
        </div>
      </MemoryRouter>
    </Provider>,
  );
}

it("shows the active feature's slug instead of the repository name", () => {
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
    screen.getByRole("button", { name: "Example repository, switch feature" }).textContent,
  ).toBe("Example repository");
});

it("shows Untitled on the kickoff route", () => {
  show({ pathname: "/new" });

  expect(screen.getByRole("button", { name: "Untitled, switch feature" }).textContent).toBe(
    "Untitled",
  );
});

it.each(["/", "/files", "/files/product/demo/index.md", "/plans/example"])(
  "shows the project on %s despite a previously active feature",
  (pathname) => {
    show({ pathname });

    expect(
      screen.getByRole("button", { name: "Example repository, switch feature" }).textContent,
    ).toBe("Example repository");
  },
);

it.each([
  "/f/Feature%20%2F%20one",
  "/f/Feature%20%2F%20one/product",
  "/f/Feature%20%2F%20one/engineering/plans",
])("uses the route's feature on %s despite a different active feature", (pathname) => {
  show({ pathname });

  expect(
    screen.getByRole("button", { name: "Switch feature, current: Feature / one" }),
  ).toBeTruthy();
});

it("uses Project before server info arrives when no feature is active", () => {
  show({ activeFeature: null, project: null });

  expect(screen.getByRole("button", { name: "Project, switch feature" }).textContent).toBe(
    "Project",
  );
});

it("opens the feature list and marks only the active feature", async () => {
  show();
  const trigger = screen.getByRole("button", { name: "Switch feature, current: demo" });

  fireEvent.click(trigger);

  const dialog = await screen.findByRole("dialog", { name: "Switch feature" });
  const active = linkTo(dialog, "/f/demo");
  const other = linkTo(dialog, "/f/Feature%20%2F%20one");
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
  expect(linkTo(dialog, "/f/demo").textContent).toContain("demo");
  expect(linkTo(dialog, "/f/Feature%20%2F%20one").textContent).toContain("Feature / one");
  expect(within(dialog).getByRole("link", { name: "Home" }).getAttribute("href")).toBe("/");
  expect(within(dialog).getByRole("link", { name: "New work…" }).getAttribute("href")).toBe("/new");
  expect(within(dialog).getByRole("separator")).toBeTruthy();
});

it("lists each feature by its slug in mono", async () => {
  show();

  fireEvent.click(screen.getByRole("button", { name: "Switch feature, current: demo" }));

  const dialog = await screen.findByRole("dialog", { name: "Switch feature" });
  const other = linkTo(dialog, "/f/Feature%20%2F%20one");
  expect(other.textContent).toBe("Feature / one");
  const slug = within(other).getByText("Feature / one").parentElement!;
  expect(slug.className).toContain("font-mono");
});

it.each(["/", "/files"])("marks no feature current on the project route %s", async (pathname) => {
  show({ pathname });
  fireEvent.click(screen.getByRole("button", { name: "Example repository, switch feature" }));

  const dialog = await screen.findByRole("dialog", { name: "Switch feature" });
  expect(dialog.querySelector("[aria-current]")).toBeNull();
  expect(dialog.querySelector("svg")).toBeNull();
});

it("marks the route's feature current despite a different active feature", async () => {
  show({ pathname: "/f/Feature%20%2F%20one/product" });
  fireEvent.click(screen.getByRole("button", { name: "Switch feature, current: Feature / one" }));

  const dialog = await screen.findByRole("dialog", { name: "Switch feature" });
  const current = linkTo(dialog, "/f/Feature%20%2F%20one");
  const previous = linkTo(dialog, "/f/demo");
  expect(current.getAttribute("aria-current")).toBe("true");
  expect(current.querySelector("svg")).not.toBeNull();
  expect(previous.getAttribute("aria-current")).toBeNull();
  expect(previous.querySelector("svg")).toBeNull();
});

it("moves focus through feature and project links with arrow keys", async () => {
  show();
  fireEvent.click(screen.getByRole("button", { name: "Switch feature, current: demo" }));
  const dialog = await screen.findByRole("dialog", { name: "Switch feature" });
  const active = linkTo(dialog, "/f/demo");
  const other = linkTo(dialog, "/f/Feature%20%2F%20one");
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

  fireEvent.keyDown(linkTo(dialog, "/f/demo"), { key: "Escape" });

  await waitFor(() => {
    expect(screen.queryByRole("dialog", { name: "Switch feature" })).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });
  expect(trigger.getAttribute("aria-expanded")).toBe("false");
});

it.each([
  ["demo", "/f/demo"],
  ["Feature / one", "/f/Feature%20%2F%20one"],
  ["Home", "/"],
  ["New work…", "/new"],
])("closes after selecting %s", async (_name, href) => {
  show();
  const trigger = screen.getByRole("button", { name: "Switch feature, current: demo" });
  fireEvent.click(trigger);
  const dialog = await screen.findByRole("dialog", { name: "Switch feature" });

  fireEvent.click(linkTo(dialog, href));

  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Switch feature" })).toBeNull());
  expect(trigger.getAttribute("aria-expanded")).toBe("false");
});

it("shows only Home and New work in an empty project", async () => {
  show({ activeFeature: null, features: [] });

  fireEvent.click(screen.getByRole("button", { name: "Example repository, switch feature" }));

  const dialog = await screen.findByRole("dialog", { name: "Switch feature" });
  expect(
    within(dialog)
      .getAllByRole("link")
      .map((link) => link.textContent),
  ).toEqual(["Home", "New work…"]);
  expect(within(dialog).queryByRole("separator")).toBeNull();
});

it("includes the visible project name in the trigger name with control characters escaped", () => {
  show({ pathname: "/files", project: "Example\u202e repository" });

  expect(
    screen.getByRole("button", { name: "Example⟨U+202E⟩ repository, switch feature" }).textContent,
  ).toBe("Example⟨U+202E⟩ repository");
});

it("renders control characters visibly in server names", async () => {
  show({ activeFeature: "demo\u202e", features: ["demo\u202e"] });

  fireEvent.click(screen.getByRole("button", { name: "Switch feature, current: demo⟨U+202E⟩" }));

  const dialog = await screen.findByRole("dialog", { name: "Switch feature" });
  expect(linkTo(dialog, "/f/demo%E2%80%AE").textContent).toContain("demo⟨U+202E⟩");
});
