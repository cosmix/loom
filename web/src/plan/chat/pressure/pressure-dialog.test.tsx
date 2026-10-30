import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import chatFixture from "@/plan/api/fixtures/chat-snapshot.json";
import hello from "@/plan/api/fixtures/hello.json";
import optionsFixture from "@/plan/api/fixtures/pressure-options.json";
import runFixture from "@/plan/api/fixtures/pressure-run.json";
import { serverInfoSchema } from "@/plan/api/schema";
import { sessionViewSchema } from "@/plan/api/schema-chat";
import { pressureOptionsSchema, type PressureOptions } from "@/plan/api/schema-pressure";
import { snapshotAtom } from "@/plan/chat/model";
import { pressureDialogAtom } from "@/plan/offers/actions-pressure";
import {
  authorBlockReasonAtom,
  authoringAtom,
  connectionAtom,
  serverInfoAtom,
  youAtom,
} from "@/plan/state/atoms";

import { PressureDialog } from "./pressure-dialog";
import { pressureOptionsAtom } from "./use-pressure";

const options = pressureOptionsSchema.parse(optionsFixture);
const plan = "doc/plans/PLAN-example.md";
const otherPlan = "doc/plans/PLAN-other.md";
const closingNotice = /first, with its closing write/;

function holderStore(value: PressureOptions = options) {
  const store = createStore();
  store.set(serverInfoAtom, { ...hello.server, read_only: [] });
  store.set(youAtom, hello.you);
  store.set(authoringAtom, { holder: { ...hello.you, away_since: null, frees_at: null } });
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: 0 });
  store.set(pressureOptionsAtom, { ok: true, value });
  store.set(pressureDialogAtom, { open: true, plan });
  initClient(store);
  return store;
}

// The form mounts only after the open-time options refetch settles, so wait for that.
async function mount(store = holderStore()) {
  render(
    <Provider store={store}>
      <MemoryRouter>
        <PressureDialog />
      </MemoryRouter>
    </Provider>,
  );
  await waitFor(() =>
    expect(screen.queryByRole("status", { name: "Loading pressure options" })).toBeNull(),
  );
  return store;
}

function jsonReply(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

// Opening the dialog refetches the options, so every test needs a fetch that answers them.
function stubFetch(replies: { options?: () => Response; start?: () => Response } = {}) {
  const spy = vi.fn<typeof fetch>(async (input) =>
    String(input).includes("/api/pressure/options")
      ? (replies.options?.() ?? jsonReply(optionsFixture))
      : (replies.start?.() ?? jsonReply(runFixture)),
  );
  vi.stubGlobal("fetch", spy);
  return spy;
}

function callsTo(spy: ReturnType<typeof stubFetch>, path: string) {
  return spy.mock.calls.filter(([input]) => String(input).includes(path));
}

beforeEach(() => {
  stubFetch();
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("prefills rounds and all six model slots from pressure options defaults", async () => {
  await mount();

  expect(screen.getByRole("button", { name: "2", pressed: true })).toBeTruthy();
  const fields = [
    ["/pressure model", options.defaults.claude_model],
    ["/pressure effort", options.defaults.claude_effort],
    ["Codex model", options.defaults.codex_model],
    ["Codex effort", options.defaults.codex_effort],
    ["/address model", options.defaults.address_model],
    ["/address effort", options.defaults.address_effort],
  ];
  for (const [label, value] of fields) {
    expect((screen.getByRole("combobox", { name: label }) as HTMLSelectElement).value).toBe(value);
  }
});

it("limits model and effort selects to their server vocabularies", async () => {
  await mount();
  const fields = [
    ["/pressure model", options.claude_models],
    ["/pressure effort", options.claude_efforts],
    ["Codex model", options.codex_models],
    ["Codex effort", options.codex_efforts],
    ["/address model", options.claude_models],
    ["/address effort", options.claude_efforts],
  ] as const;

  for (const [label, values] of fields) {
    const select = screen.getByRole("combobox", { name: label });
    expect(
      within(select)
        .getAllByRole("option")
        .map((option) => option.textContent),
    ).toEqual(values);
  }
});

it("preselects the atom plan by equality with repo-relative options", async () => {
  const listed = { ...options, plans: [plan, otherPlan] };
  stubFetch({ options: () => jsonReply(listed) });
  const store = holderStore(listed);
  store.set(pressureDialogAtom, { open: true, plan: otherPlan });

  await mount(store);

  expect((screen.getByRole("combobox", { name: "Change plan" }) as HTMLSelectElement).value).toBe(
    otherPlan,
  );
});

it("warns that an active authoring chat ends with its closing write", async () => {
  const store = holderStore();
  store.set(snapshotAtom, {
    ...store.get(snapshotAtom),
    session: sessionViewSchema.parse(chatFixture.session),
  });

  await mount(store);

  expect(screen.getByText(closingNotice)).toBeTruthy();
});

it.each(["ended", "failed", "pressure"])(
  "omits the closing-write notice for a %s session",
  async (kind) => {
    const store = holderStore();
    const session = sessionViewSchema.parse(chatFixture.session);
    store.set(snapshotAtom, {
      ...store.get(snapshotAtom),
      session:
        kind === "pressure"
          ? { ...session, kind }
          : { ...session, state: kind as "ended" | "failed" },
    });

    await mount(store);

    expect(screen.queryByText(closingNotice)).toBeNull();
  },
);

it.each([
  "The codex CLI is not installed.",
  "Codex's pressure skill is not installed under $CODEX_HOME/skills/pressure/.",
  "The server is in read-only mode.",
  "Another browser holds authoring.",
  "A pressure test is already running.",
])("disables Start with accessible options reason: %s", async (reason) => {
  const unavailable = { ...options, available: false, disabled_reason: reason };
  const fetchSpy = stubFetch({ options: () => jsonReply(unavailable) });
  await mount(holderStore(unavailable));
  const start = screen.getByRole("button", { name: "Start pressure test" });

  fireEvent.click(start);

  expect(start.getAttribute("aria-disabled")).toBe("true");
  expect(screen.getByRole("button", { name: "Start pressure test", description: reason })).toBe(
    start,
  );
  expect(start.hasAttribute("disabled")).toBe(false);
  expect(screen.getByText(reason)).toBeTruthy();
  expect(callsTo(fetchSpy, "/api/pressure/start")).toEqual([]);
});

it("gives a read-only browser an authoring reason instead of a working Start", async () => {
  const unavailable = { ...options, available: false, disabled_reason: "Other reason" };
  const store = holderStore(unavailable);
  store.set(serverInfoAtom, serverInfoSchema.parse(hello.server));
  store.set(authoringAtom, { holder: null });
  const fetchSpy = stubFetch({ options: () => jsonReply(unavailable) });
  await mount(store);
  const start = screen.getByRole("button", { name: "Start pressure test" });

  fireEvent.click(start);

  expect(start.getAttribute("aria-disabled")).toBe("true");
  expect(
    screen.getByRole("button", {
      name: "Start pressure test",
      description: "loom plan is in read-only mode: Claude is not logged in",
    }),
  ).toBe(start);
  expect(
    screen.getByRole("button", {
      name: "Start pressure test",
      description: store.get(authorBlockReasonAtom)!,
    }),
  ).toBe(start);
  expect(callsTo(fetchSpy, "/api/pressure/start")).toEqual([]);
});

it("posts the selected plan and slots and closes on success", async () => {
  const listed = { ...options, plans: [plan, otherPlan] };
  const store = holderStore(listed);
  const fetchSpy = stubFetch({ options: () => jsonReply(listed) });
  await mount(store);
  fireEvent.change(screen.getByRole("combobox", { name: "Change plan" }), {
    target: { value: otherPlan },
  });
  fireEvent.click(screen.getByRole("button", { name: "3" }));
  fireEvent.change(screen.getByRole("combobox", { name: "/pressure model" }), {
    target: { value: "opus" },
  });
  fireEvent.change(screen.getByRole("combobox", { name: "/pressure effort" }), {
    target: { value: "medium" },
  });
  fireEvent.change(screen.getByRole("combobox", { name: "Codex effort" }), {
    target: { value: "medium" },
  });
  fireEvent.change(screen.getByRole("combobox", { name: "/address model" }), {
    target: { value: "opus" },
  });
  fireEvent.change(screen.getByRole("combobox", { name: "/address effort" }), {
    target: { value: "medium" },
  });

  fireEvent.click(screen.getByRole("button", { name: "Start pressure test" }));

  await waitFor(() => expect(store.get(pressureDialogAtom).open).toBe(false));
  const starts = callsTo(fetchSpy, "/api/pressure/start");
  expect(starts).toHaveLength(1);
  const [url, request] = starts[0]!;
  expect(String(url)).toContain("/api/pressure/start");
  expect(request?.method).toBe("POST");
  expect(JSON.parse(String(request?.body))).toEqual({
    plan: otherPlan,
    rounds: 3,
    claude_model: "opus",
    claude_effort: "medium",
    codex_model: "gpt-6.1-terra",
    codex_effort: "medium",
    address_model: "opus",
    address_effort: "medium",
  });
  expect(screen.queryByRole("dialog")).toBeNull();
});

it("renders a start error as text and keeps the dialog open", async () => {
  const message = "<img src=x onerror=alert(1)>";
  stubFetch({ start: () => jsonReply({ error: "start_failed", message }, 409) });
  const store = await mount();

  fireEvent.click(screen.getByRole("button", { name: "Start pressure test" }));

  expect((await screen.findByRole("alert")).textContent).toContain(message);
  expect(document.body.querySelector("img")).toBeNull();
  expect(store.get(pressureDialogAtom).open).toBe(true);
  expect(
    screen.getByRole("button", { name: "Start pressure test" }).hasAttribute("aria-disabled"),
  ).toBe(false);
});

it("renders LoadState until the open-time refetch settles, even with cached options", async () => {
  let release: (response: Response) => void = () => {};
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>(
      () =>
        new Promise<Response>((resolve) => {
          release = resolve;
        }),
    ),
  );
  render(
    <Provider store={holderStore()}>
      <MemoryRouter>
        <PressureDialog />
      </MemoryRouter>
    </Provider>,
  );

  expect(screen.getByRole("status", { name: "Loading pressure options" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Start pressure test" })).toBeNull();

  await act(async () => {
    release(jsonReply(optionsFixture));
  });

  expect(await screen.findByRole("button", { name: "2", pressed: true })).toBeTruthy();
});

it("preselects and posts the clicked plan when only the refetched options list it", async () => {
  const cached = { ...options, plans: [otherPlan] };
  const refetched = { ...options, plans: [otherPlan, plan] };
  const fetchSpy = stubFetch({ options: () => jsonReply(refetched) });
  const store = await mount(holderStore(cached));

  expect((screen.getByRole("combobox", { name: "Change plan" }) as HTMLSelectElement).value).toBe(
    plan,
  );
  fireEvent.click(screen.getByRole("button", { name: "Start pressure test" }));

  await waitFor(() => expect(store.get(pressureDialogAtom).open).toBe(false));
  const [[, request]] = callsTo(fetchSpy, "/api/pressure/start");
  expect(JSON.parse(String(request?.body)).plan).toBe(plan);
});

it("renders a failed options response through LoadState", async () => {
  stubFetch({
    options: () => jsonReply({ error: "unavailable", message: "Try again later" }, 503),
  });
  const store = holderStore();
  store.set(pressureOptionsAtom, {
    ok: false,
    status: 503,
    error: "unavailable",
    message: "Try again later",
  });

  await mount(store);

  expect(screen.getByText("Could not load pressure options")).toBeTruthy();
  expect(screen.getByText("Try again later")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Start pressure test" })).toBeNull();
});

it("resets form defaults when reopened and cancels without posting", async () => {
  const fetchSpy = stubFetch();
  const store = await mount();
  fireEvent.click(screen.getByRole("button", { name: "3" }));
  expect(screen.getByRole("button", { name: "3", pressed: true })).toBeTruthy();

  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  await act(async () => {
    store.set(pressureDialogAtom, { open: true, plan });
  });

  expect(await screen.findByRole("button", { name: "2", pressed: true })).toBeTruthy();
  expect(callsTo(fetchSpy, "/api/pressure/start")).toEqual([]);
});

it("refetches options on every open so a newly written plan becomes selectable", async () => {
  const listed = { ...options, plans: [plan, otherPlan] };
  const fetchSpy = stubFetch({ options: () => jsonReply(listed) });
  const store = await mount();
  const select = () => screen.getByRole("combobox", { name: "Change plan" }) as HTMLSelectElement;
  const planChoices = () =>
    within(select())
      .getAllByRole("option")
      .map((option) => option.textContent);
  expect(planChoices()).toEqual([plan, otherPlan]);
  expect(callsTo(fetchSpy, "/api/pressure/options")).toHaveLength(1);
  fireEvent.change(select(), { target: { value: otherPlan } });
  expect(select().value).toBe(otherPlan);

  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  await act(async () => {
    store.set(pressureDialogAtom, { open: true, plan });
  });
  await waitFor(() => expect(callsTo(fetchSpy, "/api/pressure/options")).toHaveLength(2));
});
