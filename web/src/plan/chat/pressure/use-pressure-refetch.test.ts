import { createStore } from "jotai/vanilla";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import optionsFixture from "@/plan/api/fixtures/pressure-options.json";
import runFixture from "@/plan/api/fixtures/pressure-run.json";
import { pressureOptionsSchema, pressureRunSchema } from "@/plan/api/schema-pressure";
import { authoringAtom } from "@/plan/state/atoms";

import { socketHarness } from "./pressure-test-harness";
import { installPressureSubscription, pressureOptionsAtom, pressureRunAtom } from "./use-pressure";

const options = pressureOptionsSchema.parse(optionsFixture);
const run = pressureRunSchema.parse(runFixture);
const disposers: (() => void)[] = [];

beforeEach(() => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => Response.json(options)),
  );
});

afterEach(() => {
  disposers.splice(0).forEach((dispose) => dispose());
  vi.unstubAllGlobals();
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
const recorded = { kind: "recorded", by: null, step: null, reason: null, line: null } as const;

it("refetches options when a run starts and when it ends, not on later frames of the run", async () => {
  const busy = {
    ...options,
    available: false,
    disabled_reason: "A pressure test is already running.",
    running: true,
  };
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(Response.json(options))
    .mockResolvedValueOnce(Response.json(busy))
    .mockResolvedValueOnce(Response.json(options));
  vi.stubGlobal("fetch", fetchMock);
  const store = createStore();
  const socket = socketHarness(store);
  disposers.push(installPressureSubscription(store));
  await vi.waitFor(() =>
    expect(store.get(pressureOptionsAtom)).toEqual({ ok: true, value: options }),
  );

  socket.receive(run);
  await vi.waitFor(() => expect(store.get(pressureOptionsAtom)).toEqual({ ok: true, value: busy }));

  socket.receive({ ...run, tokens_total: run.tokens_total + 1 });
  await settle();
  expect(fetchMock).toHaveBeenCalledTimes(2);

  socket.receive({ ...run, ended: recorded });
  await vi.waitFor(() =>
    expect(store.get(pressureOptionsAtom)).toEqual({ ok: true, value: options }),
  );
  expect(fetchMock).toHaveBeenCalledTimes(3);
});

it("refetches options on pressure_options_changed with no run view, e.g. after a server restart", async () => {
  const block = {
    reason: "Codex changed session configuration: AGENTS.md",
    changed: ["AGENTS.md"],
  };
  const blocked = {
    ...options,
    available: false,
    disabled_reason: block.reason,
    config_block: block,
  };
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(Response.json(blocked))
    .mockResolvedValueOnce(Response.json(options));
  vi.stubGlobal("fetch", fetchMock);
  const store = createStore();
  const socket = socketHarness(store);
  disposers.push(installPressureSubscription(store));
  await vi.waitFor(() =>
    expect(store.get(pressureOptionsAtom)).toEqual({ ok: true, value: blocked }),
  );

  socket.optionsChanged();

  await vi.waitFor(() =>
    expect(store.get(pressureOptionsAtom)).toEqual({ ok: true, value: options }),
  );
  expect(store.get(pressureRunAtom)).toBeNull();
  expect(fetchMock).toHaveBeenCalledTimes(2);
});

it("keeps the cue's options when the ended frame's refetch answers late with a running run", async () => {
  const busy = { ...options, available: false, running: true };
  let resolveStaleBody: (value: unknown) => void = () => {};
  const staleBody = new Promise<unknown>((resolve) => {
    resolveStaleBody = resolve;
  });
  const stale = Response.json(busy);
  vi.spyOn(stale, "json").mockImplementation(() => staleBody);
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(Response.json(busy))
    .mockResolvedValueOnce(stale)
    .mockResolvedValueOnce(Response.json(options));
  vi.stubGlobal("fetch", fetchMock);
  const store = createStore();
  const socket = socketHarness(store);
  disposers.push(installPressureSubscription(store));
  await vi.waitFor(() => expect(store.get(pressureOptionsAtom)).toEqual({ ok: true, value: busy }));

  socket.receive({ ...run, ended: recorded });
  socket.optionsChanged();
  await vi.waitFor(() =>
    expect(store.get(pressureOptionsAtom)).toEqual({ ok: true, value: options }),
  );
  resolveStaleBody(busy);
  await staleBody;
  // The client parses the body first; its subscription continuation runs in the next microtask.
  await Promise.resolve();
  await settle();

  expect(store.get(pressureOptionsAtom)).toEqual({ ok: true, value: options });
  expect(fetchMock).toHaveBeenCalledTimes(3);
});

it("stops following pressure_options_changed once the subscription is disposed", async () => {
  const fetchMock = vi.fn(async () => Response.json(options));
  vi.stubGlobal("fetch", fetchMock);
  const store = createStore();
  const socket = socketHarness(store);
  const dispose = installPressureSubscription(store);
  await settle();
  const before = fetchMock.mock.calls.length;
  dispose();

  socket.optionsChanged();
  await settle();

  expect(fetchMock.mock.calls.length).toBe(before);
});

it("refetches options when a new run replaces an ended one", async () => {
  const fetchMock = vi.fn(async () => Response.json(options));
  vi.stubGlobal("fetch", fetchMock);
  const store = createStore();
  const socket = socketHarness(store);
  disposers.push(installPressureSubscription(store));
  socket.receive({ ...run, ended: recorded });
  await settle();
  const before = fetchMock.mock.calls.length;

  socket.receive({ ...run, started_ms: run.started_ms + 5000 });
  await settle();

  expect(fetchMock.mock.calls.length).toBe(before + 1);
});

it("refetches options when the authoring holder changes, not when the same holder updates", async () => {
  const notHolder = {
    ...options,
    available: false,
    disabled_reason: "Another browser holds authoring.",
  };
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(Response.json(options))
    .mockResolvedValueOnce(Response.json(notHolder))
    .mockResolvedValueOnce(Response.json(options));
  vi.stubGlobal("fetch", fetchMock);
  const store = createStore();
  socketHarness(store);
  disposers.push(installPressureSubscription(store));
  await vi.waitFor(() =>
    expect(store.get(pressureOptionsAtom)).toEqual({ ok: true, value: options }),
  );
  const ada = { public_id: "ada", name: "Ada", away_since: null, frees_at: null };

  store.set(authoringAtom, { holder: ada });
  await vi.waitFor(() =>
    expect(store.get(pressureOptionsAtom)).toEqual({ ok: true, value: notHolder }),
  );

  store.set(authoringAtom, { holder: { ...ada, away_since: "2026-01-01T00:00:00Z" } });
  await settle();
  expect(fetchMock).toHaveBeenCalledTimes(2);

  store.set(authoringAtom, { holder: null });
  await vi.waitFor(() =>
    expect(store.get(pressureOptionsAtom)).toEqual({ ok: true, value: options }),
  );
  expect(fetchMock).toHaveBeenCalledTimes(3);
});

it("stops following the authoring holder once the subscription is disposed", async () => {
  const fetchMock = vi.fn(async () => Response.json(options));
  vi.stubGlobal("fetch", fetchMock);
  const store = createStore();
  socketHarness(store);
  const dispose = installPressureSubscription(store);
  await settle();
  const before = fetchMock.mock.calls.length;
  dispose();

  store.set(authoringAtom, {
    holder: { public_id: "ada", name: "Ada", away_since: null, frees_at: null },
  });
  await settle();

  expect(fetchMock.mock.calls.length).toBe(before);
});
