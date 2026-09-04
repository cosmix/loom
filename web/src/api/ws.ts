import { snapshotSchema } from "@/api/schema";
import { applySnapshot } from "@/state/apply";
import { connectionAtom, type ConnectionPhase } from "@/state/atoms";
import type { Store } from "@/state/store";

export interface SocketDeps {
  WebSocket?: typeof WebSocket;
  fetch?: typeof fetch;
  location?: { protocol: string; host: string };
  setTimeout?: typeof setTimeout;
  clearTimeout?: typeof clearTimeout;
}

const BACKOFF_MS = [1000, 2000, 4000, 8000, 10000] as const;

function issueMessage(issues: { path: PropertyKey[]; message: string }[]): string {
  const issue = issues[0];
  const path = issue.path.map(String).join(".");
  return path ? `${path}: ${issue.message}` : issue.message;
}

class StatusSocketConnection {
  private attempts = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | undefined;
  private socket: WebSocket | undefined;
  private stopped = false;
  private readonly request: typeof fetch;
  private readonly schedule: typeof setTimeout;
  private readonly cancel: typeof clearTimeout;
  private readonly Socket: typeof WebSocket;
  private readonly url: string;
  private readonly store: Store;

  constructor(store: Store, deps: SocketDeps) {
    this.store = store;
    this.Socket = deps.WebSocket ?? globalThis.WebSocket;
    this.request = deps.fetch ?? globalThis.fetch;
    this.schedule = deps.setTimeout ?? globalThis.setTimeout;
    this.cancel = deps.clearTimeout ?? globalThis.clearTimeout;
    const location = deps.location ?? globalThis.location;
    const scheme = location.protocol === "https:" ? "wss" : "ws";
    const host = location.host;
    this.url = `${scheme}://${host}/ws`;
  }

  start(): void {
    this.setConnection("connecting");
    void this.request("/api/status", { cache: "no-store" })
      .then((response) => response.json())
      .then((snapshot) => this.receiveSnapshot(snapshot))
      .catch((error: unknown) => console.error("dashboard: status fetch failed", error));
    this.openSocket();
  }

  dispose(): void {
    this.stopped = true;
    if (this.reconnectTimer !== undefined) {
      this.cancel(this.reconnectTimer);
    }
    this.socket?.close();
  }

  private setConnection(phase: ConnectionPhase, message?: string): void {
    this.store.set(connectionAtom, {
      phase,
      since: Date.now(),
      ...(message ? { message } : {}),
    });
  }

  private openSocket(): void {
    if (this.stopped) {
      return;
    }
    const socket = new this.Socket(this.url);
    this.socket = socket;
    socket.onopen = () => {
      this.attempts = 0;
      this.setConnection("live");
    };
    socket.onmessage = (event) => this.receiveMessage(event.data);
    socket.onclose = (event) => this.reconnect(event.reason || "socket closed");
    socket.onerror = () => this.reconnect("socket error");
  }

  private receiveMessage(data: unknown): void {
    try {
      this.receiveSnapshot(JSON.parse(String(data)));
    } catch (error) {
      this.setConnection("error", "invalid JSON frame");
      console.error("dashboard: bad frame", error);
    }
  }

  private receiveSnapshot(value: unknown): void {
    const result = snapshotSchema.safeParse(value);
    if (result.success) {
      applySnapshot(this.store, result.data);
      // Only transition on phase change: `since` marks when the phase began,
      // not when the last frame arrived (connection-badge.tsx relies on that
      // distinction), so calling setConnection on every frame would stamp a
      // new `since` and break it. This also clears a stale error `message`
      // once the feed recovers.
      if (this.store.get(connectionAtom).phase !== "live") {
        this.setConnection("live");
      }
      return;
    }
    this.setConnection("error", issueMessage(result.error.issues));
    console.error("dashboard: bad frame", result.error.issues);
  }

  private reconnect(message: string): void {
    if (this.stopped || this.reconnectTimer !== undefined) {
      return;
    }
    this.setConnection("reconnecting", message);
    const delay = BACKOFF_MS[Math.min(this.attempts, BACKOFF_MS.length - 1)];
    this.attempts += 1;
    this.reconnectTimer = this.schedule(() => {
      this.reconnectTimer = undefined;
      this.openSocket();
    }, delay);
  }
}

/** Connect the dashboard to the status snapshot endpoint and live socket. */
export function connectStatusSocket(store: Store, deps: SocketDeps = {}): () => void {
  const connection = new StatusSocketConnection(store, deps);
  connection.start();
  return () => connection.dispose();
}
