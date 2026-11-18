import { createStore } from "jotai/vanilla";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import optionsFixture from "@/plan/api/fixtures/pressure-options.json";
import runFixture from "@/plan/api/fixtures/pressure-run.json";
import {
  pressureOptionsSchema,
  pressureRunSchema,
  type PressureRunView,
} from "@/plan/api/schema-pressure";
import { planSocketAtom } from "@/plan/state/atoms";

import { socketHarness } from "./pressure-test-harness";
import {
  confirmConfig,
  fetchPressureOptions,
  installPressureSubscription,
  pressureOptionsAtom,
  pressureRunAtom,
  pressureRunWaitingAtom,
  startPressure,
  stopPressure,
  type PressureStart,
} from "./use-pressure";

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

it("updates the pressure run atom from a frame without its type tag", () => {
  const store = createStore();
  const socket = socketHarness(store);
  disposers.push(installPressureSubscription(store));

  socket.receive(run);

  expect(store.get(pressureRunAtom)).toEqual(run);
});

it("refetches pressure options on resync", async () => {
  const changed = { ...options, available: false, disabled_reason: "A pressure test is running." };
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(Response.json(options))
    .mockResolvedValueOnce(Response.json(changed));
  vi.stubGlobal("fetch", fetchMock);
  const store = createStore();
  const socket = socketHarness(store);
  disposers.push(installPressureSubscription(store));
  await vi.waitFor(() =>
    expect(store.get(pressureOptionsAtom)).toEqual({ ok: true, value: options }),
  );

  socket.resync();

  await vi.waitFor(() =>
    expect(store.get(pressureOptionsAtom)).toEqual({ ok: true, value: changed }),
  );
  expect(fetchMock).toHaveBeenLastCalledWith("/api/pressure/options", {
    method: "GET",
    signal: undefined,
    credentials: "same-origin",
  });
});

it("keeps the last pressure view on resync until the latest run is replayed", () => {
  const store = createStore();
  const socket = socketHarness(store);
  disposers.push(installPressureSubscription(store));
  socket.receive(run);

  socket.resync();

  expect(store.get(pressureRunAtom)).toEqual(run);
  expect(store.get(pressureRunWaitingAtom)).toBe(true);

  const latest: PressureRunView = { ...run, step: "claude_address", tokens_total: 300 };
  socket.receive(latest);

  expect(store.get(pressureRunAtom)).toEqual(latest);
  expect(store.get(pressureRunWaitingAtom)).toBe(false);
});

it("follows socket replacement and detaches frame and resync listeners on disposal", () => {
  const store = createStore();
  const first = socketHarness(store);
  const dispose = installPressureSubscription(store);
  disposers.push(dispose);
  first.receive(run);

  store.set(planSocketAtom, null);
  const second = socketHarness(store);
  const latest: PressureRunView = { ...run, tokens_total: 300 };
  second.receive(latest);
  first.receive(run);
  first.resync();

  expect(store.get(pressureRunAtom)).toEqual(latest);
  expect(store.get(pressureRunWaitingAtom)).toBe(false);

  dispose();
  second.receive(run);
  second.resync();

  expect(store.get(pressureRunAtom)).toEqual(latest);
  expect(store.get(pressureRunWaitingAtom)).toBe(false);
});

it("does not overwrite current options with an older resync response", async () => {
  let resolveOldBody: (value: unknown) => void = () => {};
  const oldBody = new Promise<unknown>((resolve) => {
    resolveOldBody = resolve;
  });
  const oldResponse = Response.json(options);
  vi.spyOn(oldResponse, "json").mockImplementation(() => oldBody);
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(oldResponse)
    .mockResolvedValueOnce(Response.json({ ...options, running: true }));
  vi.stubGlobal("fetch", fetchMock);
  const store = createStore();
  const socket = socketHarness(store);
  disposers.push(installPressureSubscription(store));
  socket.resync();
  const latest = { ok: true, value: { ...options, running: true } };
  await vi.waitFor(() => expect(store.get(pressureOptionsAtom)).toEqual(latest));

  resolveOldBody(options);
  await oldBody;
  // The client parses the body first; its subscription continuation runs in the next microtask.
  await Promise.resolve();

  expect(store.get(pressureOptionsAtom)).toEqual(latest);
});

it("posts all pressure start slots as JSON and parses the accepted run", async () => {
  const body: PressureStart = { plan: run.plan, ...options.defaults };
  const fetchMock = vi.fn().mockResolvedValue(Response.json(run, { status: 202 }));
  vi.stubGlobal("fetch", fetchMock);

  const result = await startPressure(body);

  expect(result).toEqual({ ok: true, value: run });
  expect(fetchMock).toHaveBeenCalledWith("/api/pressure/start", {
    method: "POST",
    credentials: "same-origin",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
});

it.each([
  { action: stopPressure, path: "/api/pressure/stop", response: { stopping: true } },
  { action: confirmConfig, path: "/api/pressure/confirm-config", response: { cleared: true } },
])("parses the acknowledgement from $path", async ({ action, path, response }) => {
  const fetchMock = vi.fn().mockResolvedValue(Response.json(response));
  vi.stubGlobal("fetch", fetchMock);

  const result = await action();

  expect(result).toEqual({ ok: true, value: response });
  expect(fetchMock).toHaveBeenCalledWith(path, {
    method: "POST",
    credentials: "same-origin",
    headers: { "Content-Type": "text/plain; charset=utf-8" },
    body: "",
  });
});

it("returns a pressure start refusal through the API result", async () => {
  vi.stubGlobal(
    "fetch",
    vi
      .fn()
      .mockResolvedValue(
        Response.json(
          { error: "pressure_running", message: "A pressure test is running." },
          { status: 409 },
        ),
      ),
  );

  const result = await startPressure({ plan: run.plan, ...options.defaults });

  expect(result).toEqual({
    ok: false,
    status: 409,
    error: "pressure_running",
    message: "A pressure test is running.",
  });
});

it("rejects malformed pressure options", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(Response.json({ available: true })));

  const result = await fetchPressureOptions();

  expect(result).toMatchObject({ ok: false, error: "bad_response" });
});
