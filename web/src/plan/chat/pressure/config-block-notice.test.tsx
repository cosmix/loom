import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import hello from "@/plan/api/fixtures/hello.json";
import optionsFixture from "@/plan/api/fixtures/pressure-options.json";
import { serverInfoSchema } from "@/plan/api/schema";
import { pressureOptionsSchema, type ConfigBlock } from "@/plan/api/schema-pressure";
import { authoringAtom, connectionAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";

import { ConfigBlockNotice } from "./config-block-notice";
import { pressureOptionsAtom, pressureRunAtom } from "./use-pressure";

const block: ConfigBlock = {
  reason: "Codex changed session configuration: AGENTS.md, .codex/config.toml",
  changed: ["AGENTS.md", ".codex/config.toml"],
};
const confirmName = "I have checked these files";
const sentence = "Codex changed these session configuration files:";

function optionsWith(configBlock: ConfigBlock | null) {
  return pressureOptionsSchema.parse({ ...optionsFixture, config_block: configBlock });
}

// `free` leaves authoring unheld, as after a server restart.
function setupStore(configBlock: ConfigBlock | null = block, holder = true, free = false) {
  const store = createStore();
  store.set(serverInfoAtom, { ...hello.server, read_only: [] });
  store.set(youAtom, holder ? hello.you : { public_id: "viewer", name: "Viewer" });
  store.set(
    authoringAtom,
    free ? { holder: null } : { holder: { ...hello.you, away_since: null, frees_at: null } },
  );
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: 4000 });
  store.set(pressureOptionsAtom, { ok: true, value: optionsWith(configBlock) });
  initClient(store);
  return store;
}

function mount(store = setupStore()) {
  render(
    <Provider store={store}>
      <ConfigBlockNotice />
    </Provider>,
  );
  return store;
}

function reply(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

// POST /api/pressure/confirm-config answers `confirm`; GET /api/pressure/options answers `options`.
function stubFetch(confirm: () => Response, options: () => Response) {
  const spy = vi.fn<typeof fetch>(async (input) =>
    String(input).includes("confirm-config") ? confirm() : options(),
  );
  vi.stubGlobal("fetch", spy);
  return spy;
}

const callsTo = (spy: ReturnType<typeof stubFetch>, path: string) =>
  spy.mock.calls.filter(([input]) => String(input).includes(path));

beforeEach(() => {
  stubFetch(
    () => reply({ cleared: true }),
    () => reply(optionsWith(null)),
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("shows the persisted block with no run view, as after a server restart", () => {
  const store = mount();

  expect(store.get(pressureRunAtom)).toBeNull();
  const notice = within(screen.getByRole("region", { name: "Codex configuration change" }));
  expect(notice.getByText(sentence)).toBeTruthy();
  expect(notice.getByText("AGENTS.md")).toBeTruthy();
  expect(notice.getByText(".codex/config.toml")).toBeTruthy();
  expect(notice.getByRole("button", { name: confirmName })).toBeTruthy();
});

it("renders nothing without a block, without options or when the options failed to load", () => {
  const { container } = render(
    <Provider store={setupStore(null)}>
      <ConfigBlockNotice />
    </Provider>,
  );
  expect(container.innerHTML).toBe("");

  const store = setupStore();
  store.set(pressureOptionsAtom, null);
  const empty = render(
    <Provider store={store}>
      <ConfigBlockNotice />
    </Provider>,
  );
  expect(empty.container.innerHTML).toBe("");

  act(() =>
    store.set(pressureOptionsAtom, { ok: false, status: 500, error: "x", message: "failed" }),
  );
  expect(empty.container.innerHTML).toBe("");
});

it("posts the confirmation, refetches the options and disappears", async () => {
  const fetch = stubFetch(
    () => reply({ cleared: true }),
    () => reply(optionsWith(null)),
  );
  mount();

  fireEvent.click(screen.getByRole("button", { name: confirmName }));

  await waitFor(() => expect(screen.queryByRole("region")).toBeNull());
  expect(callsTo(fetch, "/api/pressure/confirm-config")).toHaveLength(1);
  expect(callsTo(fetch, "/api/pressure/options")).toHaveLength(1);
  expect(screen.queryByRole("button", { name: confirmName })).toBeNull();
});

it("ignores a second click while the confirmation is pending", async () => {
  let answer: (response: Response) => void = () => {};
  const fetch = vi.fn<typeof globalThis.fetch>((input) =>
    String(input).includes("confirm-config")
      ? new Promise<Response>((resolve) => {
          answer = resolve;
        })
      : Promise.resolve(reply(optionsWith(null))),
  );
  vi.stubGlobal("fetch", fetch);
  mount();
  const button = screen.getByRole("button", { name: confirmName });

  fireEvent.click(button);
  fireEvent.click(button);

  await waitFor(() => expect(callsTo(fetch, "/api/pressure/confirm-config")).toHaveLength(1));
  expect(screen.getByRole("button", { name: confirmName, description: "Confirming…" })).toBe(
    button,
  );
  await act(async () => answer(reply({ cleared: true })));
  await waitFor(() => expect(screen.queryByRole("region")).toBeNull());
  expect(callsTo(fetch, "/api/pressure/confirm-config")).toHaveLength(1);
});

it("shows a refused confirmation as text and keeps the notice while the block remains", async () => {
  const message = "<img src=x onerror=alert(1)>";
  const fetch = stubFetch(
    () => reply({ error: "internal", message }, 500),
    () => reply(optionsWith(block)),
  );
  mount();

  fireEvent.click(screen.getByRole("button", { name: confirmName }));

  expect((await screen.findByRole("alert")).textContent).toBe(message);
  expect(document.querySelector("img")).toBeNull();
  expect(screen.getByRole("region", { name: "Codex configuration change" })).toBeTruthy();
  expect(callsTo(fetch, "/api/pressure/options")).toHaveLength(1);
});

it("drops the notice when another browser already cleared the block (409)", async () => {
  stubFetch(
    () => reply({ error: "no_block", message: "No configuration change waits." }, 409),
    () => reply(optionsWith(null)),
  );
  mount();

  fireEvent.click(screen.getByRole("button", { name: confirmName }));

  await waitFor(() => expect(screen.queryByRole("region")).toBeNull());
});

it("shows the reason and paths to a browser that does not hold authoring, without the control", () => {
  mount(setupStore(block, false));

  expect(screen.getByText(sentence)).toBeTruthy();
  expect(screen.getByText("AGENTS.md")).toBeTruthy();
  expect(screen.queryByRole("button", { name: confirmName })).toBeNull();
});

it("lets a browser confirm a persisted block while nobody holds authoring", async () => {
  const fetch = stubFetch(
    () => reply({ cleared: true }),
    () => reply(optionsWith(null)),
  );
  const store = mount(setupStore(block, false, true));

  expect(store.get(pressureRunAtom)).toBeNull();
  const button = screen.getByRole("button", { name: confirmName });
  expect(button.getAttribute("aria-disabled")).toBeNull();
  fireEvent.click(button);

  await waitFor(() => expect(screen.queryByRole("region")).toBeNull());
  expect(callsTo(fetch, "/api/pressure/confirm-config")).toHaveLength(1);
});

it("hides the control while the server is read-only and nobody holds authoring", () => {
  const store = setupStore(block, false, true);
  store.set(serverInfoAtom, serverInfoSchema.parse(hello.server));
  mount(store);

  expect(screen.getByText(sentence)).toBeTruthy();
  expect(screen.queryByRole("button", { name: confirmName })).toBeNull();
});

it("hides the control while the server is read-only even for the holder", () => {
  const store = setupStore();
  store.set(serverInfoAtom, serverInfoSchema.parse(hello.server));
  mount(store);

  expect(screen.getByText(sentence)).toBeTruthy();
  expect(screen.queryByRole("button", { name: confirmName })).toBeNull();
});

it("bounds the changed-path list, wraps long paths and scrolls by keyboard", () => {
  const path = `doc/${"deeply/nested/".repeat(5)}AGENTS.md`;
  mount(setupStore({ reason: block.reason, changed: [path] }));

  const list = screen.getByRole("list", { name: "Changed files" });
  expect(list.tabIndex).toBe(0);
  for (const name of ["max-h-40", "overflow-auto", "[overflow-wrap:anywhere]"]) {
    expect(list.classList.contains(name)).toBe(true);
  }
  expect(within(list).getByText(path)).toBeTruthy();
});

it("shows a fixed sentence and the bounded list, never the reason that repeats every path", () => {
  const changed = Array.from({ length: 60 }, (_, i) => `.claude/agents/agent-${i}.md`);
  const reason = `Codex changed session configuration: ${changed.join(", ")}`;
  mount(setupStore({ reason, changed }));

  const notice = within(screen.getByRole("region", { name: "Codex configuration change" }));
  expect(notice.getByText(sentence)).toBeTruthy();
  expect(notice.getAllByRole("listitem")).toHaveLength(60);
  expect(notice.queryByText(reason)).toBeNull();
  expect(document.body.textContent).not.toContain("Codex changed session configuration");
  // Each path appears once, in the bounded list.
  expect(document.body.textContent!.split(changed[59]!)).toHaveLength(2);
});

it("shows the reason, wrapped anywhere, when the block names no files", () => {
  const reason = "Codex did not exit; its configuration changes cannot be checked";
  mount(setupStore({ reason, changed: [] }));

  const shown = screen.getByText(reason);
  expect(shown.closest("p")!.classList.contains("[overflow-wrap:anywhere]")).toBe(true);
  expect(screen.queryByText(sentence)).toBeNull();
  expect(screen.queryByRole("list")).toBeNull();
});

it("names each mounted notice by its context", () => {
  render(
    <Provider store={setupStore()}>
      <ConfigBlockNotice context="chat" />
      <ConfigBlockNotice context="plan" />
    </Provider>,
  );

  expect(screen.getByRole("region", { name: "Codex configuration change (chat)" })).toBeTruthy();
  expect(screen.getByRole("region", { name: "Codex configuration change (plan)" })).toBeTruthy();
  expect(screen.queryByRole("region", { name: "Codex configuration change" })).toBeNull();
});

it("shows control characters in the reason and paths as visible markers", () => {
  mount(setupStore({ reason: "unreadable: .claude/‮x", changed: [] }));
  expect(screen.getByText("unreadable: .claude/⟨U+202E⟩x")).toBeTruthy();
  cleanup();

  mount(setupStore({ reason: block.reason, changed: [".claude/‮x", "CLAUDE.md"] }));
  expect(screen.getAllByRole("listitem")[0]!.textContent).toBe(".claude/⟨U+202E⟩x");
  expect(document.body.textContent).not.toContain("‮");
});

it("renders markup in the reason and paths as text", () => {
  mount(setupStore({ reason: "<b>blocked</b>", changed: [] }));
  expect(screen.getByText("<b>blocked</b>")).toBeTruthy();
  cleanup();

  mount(setupStore({ reason: block.reason, changed: ["<img src=x onerror=alert(1)>"] }));
  expect(screen.getByText("<img src=x onerror=alert(1)>")).toBeTruthy();
  expect(document.querySelector("img, b")).toBeNull();
});
