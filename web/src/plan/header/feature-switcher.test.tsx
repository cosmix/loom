import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Provider } from "jotai/react";
import { createStore } from "jotai/vanilla";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import { checklistViewSchema, serverInfoSchema } from "@/plan/api/schema";
import { FeatureSwitcher } from "@/plan/header/feature-switcher";
import { activeFeatureAtom, kickoffAtom, projectAtom, serverInfoAtom } from "@/plan/state/atoms";

afterEach(cleanup);

/** The switcher's title, without the eyebrow the button also holds. */
function titleOf(button: HTMLElement): string | null {
  return button.querySelector(".plan-switcher-title")?.textContent ?? null;
}

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
  kickoff = null,
}: {
  activeFeature?: string | null;
  project?: string | null;
  features?: string[];
  pathname?: string;
  kickoff?: string | null;
} = {}) {
  const store = createStore();
  if (kickoff !== null) {
    store.set(kickoffAtom, {
      session: "kickoff",
      target: null,
      shape: {
        size: "feature",
        name: kickoff,
        proposal: false,
        problem: "",
        decisions: [],
        questions: [],
        sections: [],
      },
    });
  }
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

  expect(titleOf(screen.getByRole("button", { name: "Example repository, switch feature" }))).toBe(
    "Example repository",
  );
});

it("shows Untitled on the kickoff route", () => {
  show({ pathname: "/new" });

  expect(titleOf(screen.getByRole("button", { name: "Untitled, switch feature" }))).toBe(
    "Untitled",
  );
});

it("shows the live kickoff's name on the kickoff route", () => {
  show({ pathname: "/new", kickoff: "planner-search" });

  expect(titleOf(screen.getByRole("button", { name: "planner-search, switch feature" }))).toBe(
    "planner-search",
  );
});

it("holds the eyebrow and the title in the one switcher button", () => {
  show();

  const button = screen.getByRole("button", { name: "Switch feature, current: demo" });
  expect(button.querySelector(".plan-eyebrow")?.textContent).toBe("Planning · feature");
  expect(titleOf(button)).toBe("demo");
});

it.each(["/", "/files", "/files/product/demo/index.md", "/plans/example"])(
  "shows the project on %s despite a previously active feature",
  (pathname) => {
    show({ pathname });

    expect(
      titleOf(screen.getByRole("button", { name: "Example repository, switch feature" })),
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

  expect(titleOf(screen.getByRole("button", { name: "Project, switch feature" }))).toBe("Project");
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

it("lists features only, at encoded feature routes", async () => {
  show();

  fireEvent.click(screen.getByRole("button", { name: "Switch feature, current: demo" }));

  const dialog = await screen.findByRole("dialog", { name: "Switch feature" });
  expect(linkTo(dialog, "/f/demo").textContent).toContain("demo");
  expect(linkTo(dialog, "/f/Feature%20%2F%20one").textContent).toContain("Feature / one");
  expect(
    within(dialog)
      .getAllByRole("link")
      .map((link) => link.getAttribute("href")),
  ).toEqual(["/f/demo", "/f/Feature%20%2F%20one"]);
  expect(within(dialog).queryByRole("link", { name: "Home" })).toBeNull();
  expect(within(dialog).queryByRole("link", { name: /New work/ })).toBeNull();
  expect(within(dialog).queryByRole("separator")).toBeNull();
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
  expect(dialog.querySelector("a svg")).toBeNull();
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

it("moves focus through the feature links with arrow keys", async () => {
  show({ features: ["demo", "Feature / one", "third"] });
  fireEvent.click(screen.getByRole("button", { name: "Switch feature, current: demo" }));
  const dialog = await screen.findByRole("dialog", { name: "Switch feature" });
  const active = linkTo(dialog, "/f/demo");
  const other = linkTo(dialog, "/f/Feature%20%2F%20one");
  const third = linkTo(dialog, "/f/third");
  await waitFor(() => expect(document.activeElement).toBe(active));

  fireEvent.keyDown(active, { key: "ArrowDown" });
  expect(document.activeElement).toBe(other);
  fireEvent.keyDown(other, { key: "ArrowDown" });
  expect(document.activeElement).toBe(third);
  fireEvent.keyDown(third, { key: "ArrowDown" });
  expect(document.activeElement).toBe(active);
  fireEvent.keyDown(active, { key: "ArrowUp" });
  expect(document.activeElement).toBe(third);
  fireEvent.keyDown(third, { key: "Home" });
  expect(document.activeElement).toBe(active);
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
])("closes after selecting %s", async (_name, href) => {
  show();
  const trigger = screen.getByRole("button", { name: "Switch feature, current: demo" });
  fireEvent.click(trigger);
  const dialog = await screen.findByRole("dialog", { name: "Switch feature" });

  fireEvent.click(linkTo(dialog, href));

  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Switch feature" })).toBeNull());
  expect(trigger.getAttribute("aria-expanded")).toBe("false");
});

it("says there are no features in an empty project", async () => {
  show({ activeFeature: null, features: [] });

  fireEvent.click(screen.getByRole("button", { name: "Example repository, switch feature" }));

  const dialog = await screen.findByRole("dialog", { name: "Switch feature" });
  expect(within(dialog).queryAllByRole("link")).toEqual([]);
  expect(dialog.textContent).toContain("No features yet.");
});

it("includes the visible project name in the trigger name with control characters escaped", () => {
  show({ pathname: "/files", project: "Example\u202e repository" });

  expect(
    titleOf(screen.getByRole("button", { name: "Example⟨U+202E⟩ repository, switch feature" })),
  ).toBe("Example⟨U+202E⟩ repository");
});

it("renders control characters visibly in server names", async () => {
  show({ activeFeature: "demo\u202e", features: ["demo\u202e"] });

  fireEvent.click(screen.getByRole("button", { name: "Switch feature, current: demo⟨U+202E⟩" }));

  const dialog = await screen.findByRole("dialog", { name: "Switch feature" });
  expect(linkTo(dialog, "/f/demo%E2%80%AE").textContent).toContain("demo⟨U+202E⟩");
});
