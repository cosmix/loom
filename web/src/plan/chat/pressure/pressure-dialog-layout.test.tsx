import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import chatFixture from "@/plan/api/fixtures/chat-snapshot.json";
import docFixture from "@/plan/api/fixtures/files-doc.json";
import hello from "@/plan/api/fixtures/hello.json";
import optionsFixture from "@/plan/api/fixtures/pressure-options.json";
import runFixture from "@/plan/api/fixtures/pressure-run.json";
import { sessionViewSchema } from "@/plan/api/schema-chat";
import { pressureOptionsSchema, type PressureOptions } from "@/plan/api/schema-pressure";
import { snapshotAtom } from "@/plan/chat/model";
import { pressureDialogAtom } from "@/plan/offers/actions-pressure";
import {
  authoringAtom,
  connectionAtom,
  projectAtom,
  serverInfoAtom,
  youAtom,
} from "@/plan/state/atoms";

import { PressureDialog } from "./pressure-dialog";
import { socketHarness } from "./pressure-test-harness";
import { pressureOptionsAtom } from "./use-pressure";

const options = pressureOptionsSchema.parse(optionsFixture);
const plan = "doc/plans/PLAN-example.md";
const closingNotice = /first, with its closing write/;

function holderStore() {
  const store = createStore();
  store.set(serverInfoAtom, { ...hello.server, read_only: [] });
  store.set(youAtom, hello.you);
  store.set(authoringAtom, { holder: { ...hello.you, away_since: null, frees_at: null } });
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: 0 });
  store.set(pressureOptionsAtom, { ok: true, value: options });
  store.set(pressureDialogAtom, { open: true, plan });
  socketHarness(store);
  initClient(store);
  return store;
}

async function mount(store = holderStore()) {
  render(
    <Provider store={store}>
      <MemoryRouter>
        <PressureDialog />
      </MemoryRouter>
    </Provider>,
  );
  await screen.findByRole("group", { name: "Rounds" });
  return store;
}

function stubFetch(value: PressureOptions = options, doc?: unknown) {
  const spy = vi.fn<typeof fetch>(async (input) => {
    const url = String(input);
    const body = url.includes("/api/pressure/options")
      ? value
      : url.includes("/api/doc") && doc !== undefined
        ? doc
        : runFixture;
    return new Response(JSON.stringify(body), {
      status: 200,
      headers: { "Content-Type": "application/json" },
    });
  });
  vi.stubGlobal("fetch", spy);
  return spy;
}

beforeEach(() => {
  stubFetch();
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("explains the reviewers in the hero and preserves the consequences", async () => {
  await mount();

  expect(
    screen.getByRole("dialog", { name: "Pressure-test a plan" }).getAttribute("aria-describedby"),
  ).toBeTruthy();
  expect(
    screen.getByText("Two reviewers read the plan at once, then an editor addresses both reviews."),
  ).toBeTruthy();
  const consequences = within(screen.getByRole("list", { name: "Consequences" }));
  expect(consequences.getAllByRole("listitem")).toHaveLength(3);
  expect(consequences.getByText(/This takes a while\. Time grows with the 2 rounds/)).toBeTruthy();
  expect(
    consequences.getByText(/each round running two reviews then an address step\./),
  ).toBeTruthy();
  expect(consequences.getByText(/the plan and its brief bundle/).textContent).toBe(
    "Edits the plan and its brief bundle",
  );
  expect(consequences.getByText("planning-omissions.md").parentElement?.textContent).toBe(
    "Writes planning-omissions.md",
  );
  expect(consequences.queryByText(/minute/i)).toBeNull();
  expect(
    screen.getByText("Then a record line goes at the end of the plan: date, models, rounds."),
  ).toBeTruthy();
});

it("shows the plan-editing notice", async () => {
  await mount();

  expect(screen.getByText(/the plan and its brief bundle/)).toBeTruthy();
  expect(screen.queryByText(closingNotice)).toBeNull();
});

it("shows the standalone plan's title, the mono file name and the Change affordance", async () => {
  const store = holderStore();
  store.set(projectAtom, {
    checklists: [],
    moves: [],
    standalone: [
      { slug: "example", path: plan, state: "written", title: "Example plan", last_change: null },
    ],
  });
  await mount(store);

  expect(screen.getByRole("heading", { name: "Example plan" })).toBeTruthy();
  expect(screen.queryByText("example")).toBeNull();
  expect(screen.getByText("PLAN-example.md").closest("p")?.classList.contains("font-mono")).toBe(
    true,
  );
  expect(screen.getByText("Change")).toBeTruthy();
  expect((screen.getByRole("combobox", { name: "Change plan" }) as HTMLSelectElement).value).toBe(
    plan,
  );
});

it("falls back to the document's title, with state, change and test chips, for an unlisted plan", async () => {
  const view = {
    ...docFixture,
    feature: null,
    path: plan,
    title: "Fetched title",
    last_change: new Date(Date.now() - 3 * 3600_000).toISOString(),
    kind: "plan",
    plan: {
      slug: "example",
      state: "written",
      implements: [],
      pressure_date: null,
      last_change: null,
      validation: null,
      stages: [],
      yaml: null,
      yaml_error: null,
      metadata_line: null,
    },
  };
  stubFetch(options, view);
  await mount();

  expect(await screen.findByRole("heading", { name: "Fetched title" })).toBeTruthy();
  expect(screen.getByText("Written")).toBeTruthy();
  expect(screen.getByText("Changed 3 h ago")).toBeTruthy();
  expect(screen.getByText("Never tested")).toBeTruthy();
});

it("renders the pressure date as untrusted text, naming a format character", async () => {
  const view = {
    ...docFixture,
    feature: null,
    path: plan,
    title: "Fetched title",
    last_change: null,
    kind: "plan",
    plan: {
      slug: "example",
      state: "written",
      implements: [],
      pressure_date: "2026-10-‮01",
      last_change: null,
      validation: null,
      stages: [],
      yaml: null,
      yaml_error: null,
      metadata_line: null,
    },
  };
  stubFetch(options, view);
  await mount();

  const chip = (await screen.findByText(/^Tested/)).closest("span.inline-flex") as HTMLElement;
  expect(chip.textContent).toBe("Tested 2026-10-⟨U+202E⟩01");
  const date = within(chip).getByText("2026-10-⟨U+202E⟩01");
  expect(date.style.unicodeBidi).toBe("isolate");
});

it("names no title for a plan neither source describes, and never offers its slug as one", async () => {
  await mount();

  expect(screen.queryByRole("heading", { name: /example/i })).toBeNull();
  expect(screen.getByText("PLAN-example.md")).toBeTruthy();
});

it.each([
  { rounds: 1, legend: "Each round, once" },
  { rounds: 2, legend: "Each round, 2 times" },
  { rounds: 3, legend: "Each round, 3 times" },
])("quick-pick $rounds presses one pill and updates the diagram", async ({ rounds, legend }) => {
  const fetchSpy = stubFetch();
  await mount();

  fireEvent.click(screen.getByRole("button", { name: String(rounds) }));

  expect(screen.queryAllByRole("spinbutton")).toEqual([]);
  expect(screen.getByRole("button", { name: String(rounds), pressed: true })).toBeTruthy();
  expect(
    within(screen.getByRole("group", { name: "Rounds" })).getAllByRole("button", { pressed: true }),
  ).toHaveLength(1);
  expect(screen.getByText(legend)).toBeTruthy();
  expect(
    fetchSpy.mock.calls.filter(([input]) => String(input).includes("/api/pressure/start")),
  ).toHaveLength(0);
});

it.each([
  { node: "Review", label: "/pressure" },
  { node: "Second review", label: "Codex" },
  { node: "Address", label: "/address" },
])("keeps the $label picker fieldset in the $node diagram node", async ({ node, label }) => {
  await mount();

  const group = within(screen.getByRole("region", { name: node })).getByRole("group", {
    name: label,
  });
  expect(within(group).getByRole("combobox", { name: `${label} model` })).toBeTruthy();
  expect(within(group).getByRole("combobox", { name: `${label} effort` })).toBeTruthy();
});

it("starts with the new label and the quick-picked rounds", async () => {
  const fetchSpy = stubFetch();
  const store = await mount();
  expect(screen.getByText("Holds authoring until it finishes")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "3" }));

  fireEvent.click(screen.getByRole("button", { name: "Start pressure test" }));

  await waitFor(() => expect(store.get(pressureDialogAtom).open).toBe(false));
  const starts = fetchSpy.mock.calls.filter(([input]) =>
    String(input).includes("/api/pressure/start"),
  );
  expect(starts).toHaveLength(1);
  expect(JSON.parse(String(starts[0]![1]?.body)).rounds).toBe(3);
});

it("renders the closing-write warning as the fourth consequence for active authoring", async () => {
  const store = holderStore();
  store.set(snapshotAtom, {
    ...store.get(snapshotAtom),
    session: sessionViewSchema.parse(chatFixture.session),
  });

  await mount(store);

  const consequences = within(screen.getByRole("list", { name: "Consequences" }));
  expect(consequences.getAllByRole("listitem")).toHaveLength(4);
  expect(
    consequences.getByText(closingNotice).closest("li")?.classList.contains("tone-warning"),
  ).toBe(true);
});

it.each(["none", "ended", "failed", "pressure"])(
  "omits the warning consequence for a %s session",
  async (kind) => {
    const store = holderStore();
    const session = sessionViewSchema.parse(chatFixture.session);
    store.set(snapshotAtom, {
      ...store.get(snapshotAtom),
      session:
        kind === "none"
          ? null
          : kind === "pressure"
            ? { ...session, kind }
            : { ...session, state: kind as "ended" | "failed" },
    });

    await mount(store);

    expect(screen.queryByText(closingNotice)).toBeNull();
    expect(
      within(screen.getByRole("list", { name: "Consequences" })).getAllByRole("listitem"),
    ).toHaveLength(3);
  },
);

it("renders control characters in the plan title and file name as visible markers", async () => {
  const evil = "doc/plans/PLAN-\u202Eevil.md";
  stubFetch({ ...options, plans: [evil] });
  const store = holderStore();
  store.set(projectAtom, {
    checklists: [],
    moves: [],
    standalone: [
      { slug: "evil", path: evil, state: "written", title: "Evil\u202E plan", last_change: null },
    ],
  });
  await mount(store);

  expect(screen.getByRole("heading", { name: "Evil⟨U+202E⟩ plan" })).toBeTruthy();
  expect(screen.getByText("PLAN-⟨U+202E⟩evil.md")).toBeTruthy();
  expect(document.body.textContent).not.toContain("\u202E");
});
