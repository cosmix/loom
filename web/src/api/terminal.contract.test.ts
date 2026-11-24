import { beforeEach, describe, expect, it } from "vitest";

import * as terminal from "@/api/terminal";
import type { TerminalDeps, TerminalState } from "@/api/terminal";

class FakeSocket {
  static readonly OPEN = 1;
  static instances: FakeSocket[] = [];

  binaryType: BinaryType = "blob";
  closed = false;
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
    this.closed = true;
    this.readyState = 3;
  }

  open(): void {
    this.readyState = FakeSocket.OPEN;
    this.onopen?.(new Event("open"));
  }

  send(data: unknown): void {
    this.sent.push(data);
  }

  receive(data: ArrayBuffer | string): void {
    this.onmessage?.({ data } as MessageEvent);
  }

  closeFromServer(code: number, reason: string): void {
    this.readyState = 3;
    this.onclose?.({ code, reason } as CloseEvent);
  }
}

interface ScheduledTimer {
  id: number;
  callback: () => void;
  delay: number;
  cancelled: boolean;
}

let nextTimerId = 1;
let timers: ScheduledTimer[] = [];

function fakeTimeout(handler: TimerHandler, delay = 0): number {
  const timer = { id: nextTimerId++, callback: handler as () => void, delay, cancelled: false };
  timers.push(timer);
  return timer.id;
}

function fakeClearTimeout(id: number): void {
  const timer = timers.find((candidate) => candidate.id === id);
  if (timer) timer.cancelled = true;
}

function pendingTimers(): ScheduledTimer[] {
  return timers.filter((timer) => !timer.cancelled);
}

function dependencies(): TerminalDeps {
  let timestamp = 0;
  return {
    WebSocket: FakeSocket as unknown as typeof WebSocket,
    location: { protocol: "http:", host: "127.0.0.1:7373" },
    setTimeout: fakeTimeout as unknown as typeof setTimeout,
    clearTimeout: fakeClearTimeout as unknown as typeof clearTimeout,
    now: () => ++timestamp,
  };
}

function connect(): TerminalState[] {
  const states: TerminalState[] = [];
  new terminal.TerminalConnection(
    "stage/a",
    "control",
    () => {},
    (state) => states.push(state),
    dependencies(),
  );
  return states;
}

const latestSocket = () => FakeSocket.instances[FakeSocket.instances.length - 1];
const ATTACH_FAILED = "could not start the tmux attach client";

beforeEach(() => {
  FakeSocket.instances = [];
  timers = [];
  nextTimerId = 1;
});

describe("terminal startup contract", () => {
  it("reports live only after the ready frame and gives up after repeated attach failures", () => {
    expect(Reflect.get(terminal, "STARTUP_TIMEOUT_MS")).toBe(10000);

    const ready = connect();
    latestSocket().open();
    expect(ready.at(-1)?.phase).toBe("connecting");
    expect(ready.some((state) => state.phase === "live")).toBe(false);
    latestSocket().receive('{"ready":true}');
    expect(ready.at(-1)?.phase).toBe("live");

    // Part b runs on a fresh connection; only its timers are counted.
    timers = [];
    const failing = connect();
    latestSocket().open();
    latestSocket().closeFromServer(4010, ATTACH_FAILED);
    const retries = pendingTimers();
    expect(retries).toHaveLength(1);
    retries[0].cancelled = true;
    retries[0].callback();
    latestSocket().open();
    latestSocket().closeFromServer(4010, ATTACH_FAILED);

    expect(failing.some((state) => state.phase === "live")).toBe(false);
    expect(failing.at(-1)).toMatchObject({ phase: "dropped", reason: ATTACH_FAILED });
    expect(pendingTimers()).toEqual([]);
  });

  it("shows close code 4011 as refused with the server reason and no retry", () => {
    const states = connect();
    latestSocket().open();
    latestSocket().closeFromServer(4011, "terminal limit reached");

    expect(states.at(-1)).toMatchObject({ phase: "refused", reason: "terminal limit reached" });
    expect(states.at(-1)?.reason).not.toBe("handshake refused");
    expect(pendingTimers()).toEqual([]);
  });
});
