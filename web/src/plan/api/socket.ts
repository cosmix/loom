import { apiGet } from "@/plan/api/client";
import {
  socketMessageSchema,
  serverInfoSchema,
  type ClientMessage,
  type PlanMessage,
} from "@/plan/api/schema";
import {
  authoringAtom,
  checklistAtom,
  connectionAtom,
  connectionEpochAtom,
  presenceAtom,
  serverInfoAtom,
  stoppingAtom,
  unauthorizedAtom,
  youAtom,
} from "@/plan/state/atoms";
import type { Store } from "@/state/store";

type PlanMessageType = PlanMessage["type"];
type Phase = "connecting" | "open" | "reconnecting" | "unauthorized";
type HelloMessage = Extract<PlanMessage, { type: "hello" }>;
type SocketDeps = {
  WebSocketImpl?: typeof WebSocket;
  setTimeout?: typeof setTimeout;
  clearTimeout?: typeof clearTimeout;
};
const DELAYS = [1000, 2000, 5000, 10000] as const;
const OPEN = 1;

class PlanSocketConnection {
  private readonly store: Store;
  private readonly Socket: typeof WebSocket;
  private readonly schedule: typeof setTimeout;
  private readonly cancel: typeof clearTimeout;
  private readonly url: string;
  private readonly handlers = new Set<(message: PlanMessage) => void>();
  private readonly resyncHandlers = new Set<() => void>();
  private socket: WebSocket | undefined;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private stopped = false;
  private attempts = 0;
  private receivedHello = false;

  constructor(store: Store, deps: SocketDeps) {
    this.store = store;
    this.Socket = deps.WebSocketImpl ?? globalThis.WebSocket;
    // Bound: native timers called as `this.schedule(...)` run with `this` set to
    // this instance instead of `globalThis`, which browsers reject.
    this.schedule = deps.setTimeout ?? globalThis.setTimeout.bind(globalThis);
    this.cancel = deps.clearTimeout ?? globalThis.clearTimeout.bind(globalThis);
    const scheme = globalThis.location.protocol === "https:" ? "wss" : "ws";
    this.url = `${scheme}://${globalThis.location.host}/ws`;
  }

  start(): void {
    this.setPhase("connecting");
    void this.probe();
  }

  send(message: ClientMessage): boolean {
    const socket = this.socket;
    if (this.store.get(connectionAtom).phase !== "open" || !socket || socket.readyState !== OPEN) {
      return false;
    }
    socket.send(JSON.stringify(message));
    return true;
  }

  subscribe<K extends PlanMessageType>(
    type: K,
    handler: (message: Extract<PlanMessage, { type: K }>) => void,
  ): () => void {
    const dispatch = (message: PlanMessage) => {
      if (message.type === type) handler(message as Extract<PlanMessage, { type: K }>);
    };
    this.handlers.add(dispatch);
    return () => {
      this.handlers.delete(dispatch);
    };
  }

  onResync(handler: () => void): () => void {
    this.resyncHandlers.add(handler);
    return () => {
      this.resyncHandlers.delete(handler);
    };
  }

  dispose(): void {
    this.stopped = true;
    if (this.timer !== undefined) this.cancel(this.timer);
    this.socket?.close();
    this.socket = undefined;
    this.handlers.clear();
    this.resyncHandlers.clear();
  }

  private setPhase(next: Phase): void {
    this.store.set(connectionAtom, (current) => ({
      ...current,
      phase: next,
      everOpen: current.everOpen || next === "open",
    }));
  }

  private retry(): void {
    if (this.stopped || this.timer !== undefined) return;
    this.setPhase("reconnecting");
    const delay = DELAYS[Math.min(this.attempts, DELAYS.length - 1)];
    this.attempts += 1;
    this.timer = this.schedule(() => {
      this.timer = undefined;
      void this.probe();
    }, delay);
  }

  private applyHello(message: HelloMessage): void {
    this.store.set(youAtom, message.you);
    this.store.set(serverInfoAtom, message.server);
    this.store.set(checklistAtom, message.checklist);
    this.store.set(presenceAtom, message.presence);
    this.store.set(authoringAtom, message.authoring);
    this.store.set(stoppingAtom, message.server.stopping);
    this.attempts = 0;
    // Atoms first, then the epoch, then the resync callbacks: a callback that
    // refetches must see the new snapshot and the bumped epoch.
    if (this.receivedHello) {
      this.store.set(connectionEpochAtom, (epoch) => epoch + 1);
      this.resyncHandlers.forEach((handler) => handler());
    }
    this.receivedHello = true;
  }

  private apply(message: PlanMessage): void {
    switch (message.type) {
      case "hello":
        this.applyHello(message);
        break;
      case "presence":
        this.store.set(presenceAtom, { entries: message.entries });
        break;
      case "authoring":
        this.store.set(authoringAtom, { holder: message.holder });
        break;
      case "checklist": {
        const { type: _type, ...checklist } = message;
        this.store.set(checklistAtom, checklist);
        break;
      }
      case "server_stopping":
        this.store.set(stoppingAtom, true);
        break;
      case "refused":
        break;
    }
  }

  private receive(data: unknown): void {
    let value: unknown;
    try {
      value = JSON.parse(String(data));
    } catch (error) {
      console.warn("plan socket: invalid frame", error);
      return;
    }
    const parsed = socketMessageSchema.safeParse(value);
    if (!parsed.success) {
      console.warn("plan socket: invalid frame", parsed.error);
      return;
    }
    this.store.set(connectionAtom, (current) => ({ ...current, lastMessageAt: Date.now() }));
    this.apply(parsed.data);
    this.handlers.forEach((handler) => handler(parsed.data));
  }

  private openSocket(): void {
    let current: WebSocket;
    try {
      current = new this.Socket(this.url);
    } catch {
      this.retry();
      return;
    }
    this.socket = current;
    current.onopen = () => {
      if (!this.stopped && this.socket === current) this.setPhase("open");
    };
    current.onmessage = (event) => {
      if (!this.stopped && this.socket === current) this.receive(event.data);
    };
    current.onclose = () => {
      if (this.stopped || this.socket !== current) return;
      this.socket = undefined;
      this.retry();
    };
    current.onerror = () => {
      if (this.stopped || this.socket !== current) return;
      this.socket = undefined;
      current.close();
      this.retry();
    };
  }

  private async probe(): Promise<void> {
    const result = await apiGet("/api/server", serverInfoSchema);
    if (this.stopped) return;
    if (!result.ok) {
      if (result.status === 401) {
        this.store.set(unauthorizedAtom, true);
        this.setPhase("unauthorized");
      } else this.retry();
      return;
    }
    this.openSocket();
  }
}

export function connectPlanSocket(store: Store, deps: SocketDeps = {}) {
  const connection = new PlanSocketConnection(store, deps);
  connection.start();
  // Arrow members, not the class instance: callers may destructure the handle.
  return {
    send: (message: ClientMessage): boolean => connection.send(message),
    subscribe: <K extends PlanMessageType>(
      type: K,
      handler: (message: Extract<PlanMessage, { type: K }>) => void,
    ): (() => void) => connection.subscribe(type, handler),
    onResync: (handler: () => void): (() => void) => connection.onResync(handler),
    dispose: (): void => connection.dispose(),
  };
}
