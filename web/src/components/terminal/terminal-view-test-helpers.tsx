import { act, render, screen, waitFor } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider, useAtomValue } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { expect } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type Snapshot, type StageSummary } from "@/api/schema";
import { StageModal } from "@/components/stage-modal";
import { TerminalView } from "@/components/terminal/terminal-view";
import type { Emulator, EmulatorFactory, TerminalDeps } from "@/components/terminal/use-terminal";
import { TooltipProvider } from "@/components/ui/tooltip";
import { applySnapshot } from "@/state/apply";
import { selectStage, snapshotAtom } from "@/state/atoms";
const fixture = snapshotSchema.parse(fixtureJson);

export class FakeSocket {
  static readonly OPEN = 1;
  static instances: FakeSocket[] = [];
  binaryType: BinaryType = "blob";
  onclose: ((event: CloseEvent) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onopen: ((event: Event) => void) | null = null;
  readyState = 0;
  readonly sent: unknown[] = [];
  readonly url: string;
  constructor(url: string) {
    this.url = url;
    FakeSocket.instances.push(this);
  }
  close(): void {
    this.readyState = 3;
  }
  open(): void {
    this.readyState = FakeSocket.OPEN;
    this.onopen?.(new Event("open"));
  }
  send(data: string | ArrayBufferLike | Blob | ArrayBufferView): void {
    this.sent.push(data);
  }
  closeFromServer(code: number, reason = "server reason"): void {
    this.readyState = 3;
    this.onclose?.({ code, reason } as CloseEvent);
  }
}
interface ScheduledTimer {
  callback: () => void;
  cancelled: boolean;
  delay: number;
  id: number;
}
export function fakeTimers() {
  const all: ScheduledTimer[] = [];
  let nextId = 1;
  const schedule = (callback: TimerHandler, delay = 0): number => {
    all.push({ callback: callback as () => void, cancelled: false, delay, id: nextId });
    nextId += 1;
    return nextId - 1;
  };
  const cancel = (id: number): void => {
    const timer = all.find((candidate) => candidate.id === id);
    if (timer) timer.cancelled = true;
  };
  return {
    all,
    active: () => all.filter((timer) => !timer.cancelled),
    deps: {
      WebSocket: FakeSocket as unknown as typeof WebSocket,
      location: { protocol: "http:", host: "127.0.0.1:7373" },
      setTimeout: schedule as unknown as typeof setTimeout,
      clearTimeout: cancel as unknown as typeof clearTimeout,
    } satisfies TerminalDeps,
    fireNext: () => {
      const timer = all.find((candidate) => !candidate.cancelled);
      if (!timer) throw new Error("expected a scheduled timer");
      timer.cancelled = true;
      timer.callback();
    },
  };
}
interface EmulatorDouble {
  calls: {
    disposed: number;
    fit: number;
    focused: number;
    opened: HTMLElement[];
    readOnly: boolean[];
    written: Uint8Array[];
  };
  emulator: Emulator;
  emit(data: string): void;
  scroll(pages: number): void;
}
export function fakeEmulator(): EmulatorDouble {
  let onData: ((data: string) => void) | undefined;
  let onScroll: ((event: { pages: number }) => void) | undefined;
  const calls: EmulatorDouble["calls"] = {
    disposed: 0,
    fit: 0,
    focused: 0,
    opened: [],
    readOnly: [],
    written: [],
  };
  return {
    calls,
    emulator: {
      open(host) {
        calls.opened.push(host);
      },
      write(data) {
        calls.written.push(data);
      },
      onData(handler) {
        onData = handler;
        return () => {
          if (onData === handler) onData = undefined;
        };
      },
      fit() {
        calls.fit += 1;
        return { cols: 120, rows: 36 };
      },
      focus() {
        calls.focused += 1;
      },
      setReadOnly(readOnly) {
        calls.readOnly.push(readOnly);
      },
      onScroll(handler) {
        onScroll = handler;
        return () => {
          onScroll = undefined;
        };
      },
      dispose() {
        calls.disposed += 1;
      },
    },
    emit(data) {
      onData?.(data);
    },
    scroll(pages) {
      onScroll?.({ pages });
    },
  };
}
export function fakeFactory() {
  const doubles: EmulatorDouble[] = [];
  return {
    doubles,
    factory: async () => {
      const next = fakeEmulator();
      doubles.push(next);
      return next.emulator;
    },
  };
}
export function terminalStage(overrides: Partial<StageSummary> = {}): StageSummary {
  const source = fixture.status.stages.find((stage) => stage.id === "client");
  if (!source) throw new Error("fixture stage client is missing");
  return { ...source, session_alive: true, session_backend: "tmux", ...overrides };
}
export function snapshotFor(stage: StageSummary, terminals = true): Snapshot {
  return {
    ...structuredClone(fixture),
    terminals,
    status: {
      ...fixture.status,
      stages: [stage, ...fixture.status.stages.filter((candidate) => candidate.id !== "client")],
    },
  };
}
/// Reads its stage from the snapshot atom, like `StageModal` does, so a test
/// that pushes a later snapshot into the store (eg. `session_alive` flipping
/// back on) actually reaches `TerminalView` instead of the stage it was
/// first rendered with.
function LiveStage({
  id,
  factory,
  deps,
}: {
  id: string;
  factory: EmulatorFactory;
  deps: TerminalDeps;
}) {
  const snapshot = useAtomValue(snapshotAtom);
  const stage = selectStage(snapshot, id);
  if (!stage) throw new Error(`stage ${id} missing from snapshot`);
  return <TerminalView stage={stage} frame="page" factory={factory} deps={deps} />;
}

export function renderView(stage: StageSummary, factory: EmulatorFactory, deps: TerminalDeps) {
  const store = createStore();
  applySnapshot(store, snapshotFor(stage));
  return {
    store,
    ...render(
      <Provider store={store}>
        <TooltipProvider>
          <LiveStage id={stage.id} factory={factory} deps={deps} />
        </TooltipProvider>
      </Provider>,
    ),
  };
}
export function renderModal(
  stage: StageSummary,
  factory: EmulatorFactory,
  deps: TerminalDeps,
  path: string,
) {
  const store = createStore();
  applySnapshot(store, snapshotFor(stage));
  const router = createMemoryRouter(
    [{ path: "/", element: <StageModal terminalFactory={factory} terminalDeps={deps} /> }],
    { initialEntries: [path] },
  );
  render(
    <Provider store={store}>
      <TooltipProvider>
        <RouterProvider router={router} />
      </TooltipProvider>
    </Provider>,
  );
  return { router, store };
}
export function well(): HTMLElement {
  const element = screen.getByLabelText("Agent terminal").closest(".terminal-well");
  if (!(element instanceof HTMLElement)) throw new Error("terminal well is missing");
  return element;
}
export async function waitForMount(
  factory: ReturnType<typeof fakeFactory>,
): Promise<EmulatorDouble> {
  await waitFor(() => expect(factory.doubles).toHaveLength(1));
  return factory.doubles[0];
}
export async function mounted(stage = terminalStage()) {
  const timers = fakeTimers();
  const factory = fakeFactory();
  const rendered = renderView(stage, factory.factory, timers.deps);
  return { ...rendered, emulator: await waitForMount(factory), factory, timers };
}

export async function live(stage = terminalStage()) {
  const terminal = await mounted(stage);
  act(() => FakeSocket.instances[0].open());
  return terminal;
}
