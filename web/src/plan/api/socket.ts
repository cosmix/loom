import { apiGet } from "@/plan/api/client";
import {
  planMessageSchema,
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
type SocketDeps = {
  WebSocketImpl?: typeof WebSocket;
  setTimeout?: typeof setTimeout;
  clearTimeout?: typeof clearTimeout;
};
const DELAYS = [1000, 2000, 5000, 10000] as const;

export function connectPlanSocket(store: Store, deps: SocketDeps = {}) {
  const Socket = deps.WebSocketImpl ?? globalThis.WebSocket;
  const schedule = deps.setTimeout ?? globalThis.setTimeout.bind(globalThis);
  const cancel = deps.clearTimeout ?? globalThis.clearTimeout.bind(globalThis);
  const scheme = globalThis.location.protocol === "https:" ? "wss" : "ws";
  const url = `${scheme}://${globalThis.location.host}/ws`;
  const handlers = new Set<(message: PlanMessage) => void>();
  const resyncHandlers = new Set<() => void>();
  let socket: WebSocket | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let stopped = false;
  let attempts = 0;
  let receivedHello = false;

  function phase(next: "connecting" | "open" | "reconnecting" | "unauthorized") {
    store.set(connectionAtom, (current) => ({
      ...current,
      phase: next,
      everOpen: current.everOpen || next === "open",
    }));
  }

  function retry() {
    if (stopped || timer !== undefined) return;
    phase("reconnecting");
    const delay = DELAYS[Math.min(attempts, DELAYS.length - 1)];
    attempts += 1;
    timer = schedule(() => {
      timer = undefined;
      void probe();
    }, delay);
  }

  function apply(message: PlanMessage) {
    switch (message.type) {
      case "hello":
        store.set(youAtom, message.you);
        store.set(serverInfoAtom, message.server);
        store.set(checklistAtom, message.checklist);
        store.set(presenceAtom, message.presence);
        store.set(authoringAtom, message.authoring);
        store.set(stoppingAtom, message.server.stopping);
        attempts = 0;
        if (receivedHello) {
          store.set(connectionEpochAtom, (epoch) => epoch + 1);
          resyncHandlers.forEach((handler) => handler());
        }
        receivedHello = true;
        break;
      case "presence":
        store.set(presenceAtom, { entries: message.entries });
        break;
      case "authoring":
        store.set(authoringAtom, { holder: message.holder });
        break;
      case "checklist": {
        const { type: _type, ...checklist } = message;
        store.set(checklistAtom, checklist);
        break;
      }
      case "server_stopping":
        store.set(stoppingAtom, true);
        break;
      case "refused":
        break;
    }
  }

  function receive(data: unknown) {
    let value: unknown;
    try {
      value = JSON.parse(String(data));
    } catch (error) {
      console.warn("plan socket: invalid frame", error);
      return;
    }
    const parsed = planMessageSchema.safeParse(value);
    if (!parsed.success) {
      console.warn("plan socket: invalid frame", parsed.error);
      return;
    }
    store.set(connectionAtom, (current) => ({ ...current, lastMessageAt: Date.now() }));
    apply(parsed.data);
    handlers.forEach((handler) => handler(parsed.data));
  }

  function openSocket() {
    try {
      socket = new Socket(url);
    } catch {
      retry();
      return;
    }
    const current = socket;
    current.onopen = () => {
      if (!stopped && socket === current) phase("open");
    };
    current.onmessage = (event) => {
      if (!stopped && socket === current) receive(event.data);
    };
    current.onclose = () => {
      if (stopped || socket !== current) return;
      socket = undefined;
      retry();
    };
    current.onerror = () => {
      if (stopped || socket !== current) return;
      socket = undefined;
      current.close();
      retry();
    };
  }

  async function probe() {
    const result = await apiGet("/api/server", serverInfoSchema);
    if (stopped) return;
    if (!result.ok) {
      if (result.status === 401) {
        store.set(unauthorizedAtom, true);
        phase("unauthorized");
      } else retry();
      return;
    }
    openSocket();
  }

  phase("connecting");
  void probe();
  return {
    send(message: ClientMessage): boolean {
      if (store.get(connectionAtom).phase !== "open" || !socket || socket.readyState !== 1)
        return false;
      socket.send(JSON.stringify(message));
      return true;
    },
    subscribe<K extends PlanMessageType>(
      type: K,
      handler: (message: Extract<PlanMessage, { type: K }>) => void,
    ): () => void {
      const dispatch = (message: PlanMessage) => {
        if (message.type === type) handler(message as Extract<PlanMessage, { type: K }>);
      };
      handlers.add(dispatch);
      return () => {
        handlers.delete(dispatch);
      };
    },
    onResync(handler: () => void): () => void {
      resyncHandlers.add(handler);
      return () => {
        resyncHandlers.delete(handler);
      };
    },
    dispose(): void {
      stopped = true;
      if (timer !== undefined) cancel(timer);
      socket?.close();
      socket = undefined;
      handlers.clear();
      resyncHandlers.clear();
    },
  };
}
