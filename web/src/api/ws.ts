import { parseSnapshot } from "@/api/schema";
import { applySnapshot } from "@/state/apply";
import { connectionAtom, type ConnectionPhase } from "@/state/atoms";
import type { Store } from "@/state/store";

export interface SocketDeps {
  WebSocket?: typeof WebSocket;
  fetch?: typeof fetch;
  location?: { protocol: string; host: string };
  setTimeout?: typeof setTimeout;
  clearTimeout?: typeof clearTimeout;
  setInterval?: typeof setInterval;
  clearInterval?: typeof clearInterval;
  now?: () => number;
}

// The server resends the last frame every 10s of silence (loom ws.rs
// KEEPALIVE), so 30s without any message means the socket is dead even though
// no close event fired (a half-open TCP connection never fires one).
const WATCHDOG_CHECK_MS = 5000;
const STALE_AFTER_MS = 30000;
const BACKOFF_MS = [1000, 2000, 4000, 8000, 10000] as const;
// Not a timer: a cumulative-delay floor. By the 5th consecutive reconnect
// failure the outage has already burned through the first four backoff
// delays (1s+2s+4s+8s = 15s), so from that failure on the feed is stale
// enough that showing it as merely "reconnecting" is misleading - it's
// "offline" until a socket actually opens again.
const OFFLINE_AFTER_ATTEMPTS = 4;

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
  private generation = 0;
  private readonly request: typeof fetch;
  private readonly schedule: typeof setTimeout;
  private readonly cancel: typeof clearTimeout;
  private watchdog: ReturnType<typeof setInterval> | undefined;
  private lastMessageAt = 0;
  private readonly every: typeof setInterval;
  private readonly stopEvery: typeof clearInterval;
  private readonly now: () => number;
  private readonly Socket: typeof WebSocket;
  private readonly url: string;
  private readonly store: Store;

  constructor(store: Store, deps: SocketDeps) {
    this.store = store;
    this.Socket = deps.WebSocket ?? globalThis.WebSocket;
    // Bound: an unbound `globalThis.fetch`/`setTimeout`/`clearTimeout` called
    // as `this.request(...)` etc. runs with `this` set to the client
    // instance rather than `globalThis`, which native implementations reject
    // with "Illegal invocation" in a real browser (only mocked in jsdom/tests).
    this.request = deps.fetch ?? globalThis.fetch.bind(globalThis);
    this.schedule = deps.setTimeout ?? globalThis.setTimeout.bind(globalThis);
    this.cancel = deps.clearTimeout ?? globalThis.clearTimeout.bind(globalThis);
    this.every = deps.setInterval ?? globalThis.setInterval.bind(globalThis);
    this.stopEvery = deps.clearInterval ?? globalThis.clearInterval.bind(globalThis);
    this.now = deps.now ?? Date.now;
    const location = deps.location ?? globalThis.location;
    const scheme = location.protocol === "https:" ? "wss" : "ws";
    const host = location.host;
    this.url = `${scheme}://${host}/ws`;
  }

  start(): void {
    this.setConnection("connecting");
    this.openSocket();
    this.watchdog = this.every(() => this.checkLiveness(), WATCHDOG_CHECK_MS);
    // Background tabs throttle timers, so also check as soon as the tab is shown.
    globalThis.document?.addEventListener("visibilitychange", this.onVisibility);
    const generation = this.generation;
    void this.request("/api/status", { cache: "no-store" })
      .then((response) => response.json())
      .then((snapshot) => {
        if (!this.stopped && this.generation === generation) {
          this.receiveSnapshot(snapshot, false);
        }
      })
      .catch((error: unknown) => {
        if (!this.stopped && this.generation === generation) {
          console.error("dashboard: status fetch failed", error);
        }
      });
  }

  dispose(): void {
    this.stopped = true;
    this.generation += 1;
    if (this.reconnectTimer !== undefined) {
      this.cancel(this.reconnectTimer);
    }
    if (this.watchdog !== undefined) {
      this.stopEvery(this.watchdog);
      this.watchdog = undefined;
    }
    globalThis.document?.removeEventListener("visibilitychange", this.onVisibility);
    this.socket?.close();
  }

  private readonly onVisibility = (): void => {
    if (globalThis.document.visibilityState === "visible") {
      this.checkLiveness();
    }
  };

  private checkLiveness(): void {
    if (this.stopped || this.socket === undefined) return;
    const phase = this.store.get(connectionAtom).phase;
    if (
      (phase === "live" || phase === "error") &&
      this.now() - this.lastMessageAt > STALE_AFTER_MS
    ) {
      this.reconnect("no data for 30s");
    }
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
    // Every handler checks it's still the current socket: a superseded
    // socket can fire onclose/onerror after a later reconnect has already
    // opened a fresh one, and reacting to that stale event would schedule a
    // second, untracked reconnect loop.
    socket.onopen = () => {
      if (this.stopped || this.socket !== socket) return;
      this.attempts = 0;
      this.lastMessageAt = this.now();
      this.setConnection("live");
    };
    socket.onmessage = (event) => {
      if (this.stopped || this.socket !== socket) return;
      // Before parsing: even a bad frame proves the connection is alive.
      this.lastMessageAt = this.now();
      this.receiveMessage(event.data);
    };
    socket.onclose = (event) => {
      if (this.stopped || this.socket !== socket) return;
      this.reconnect(event.reason || "socket closed");
    };
    socket.onerror = () => {
      if (this.stopped || this.socket !== socket) return;
      this.reconnect("socket error");
    };
  }

  private receiveMessage(data: unknown): void {
    try {
      this.receiveSnapshot(JSON.parse(String(data)));
    } catch (error) {
      this.setConnection("error", "invalid JSON frame");
      console.error("dashboard: bad frame", error);
    }
  }

  private receiveSnapshot(value: unknown, live = true): void {
    const result = parseSnapshot(value);
    if (result.success) {
      applySnapshot(this.store, result.data);
      // Only transition on phase change: `since` marks when the phase began,
      // not when the last frame arrived (connection-badge.tsx relies on that
      // distinction), so calling setConnection on every frame would stamp a
      // new `since` and break it. This also clears a stale error `message`
      // once the feed recovers.
      if (live && this.store.get(connectionAtom).phase !== "live") {
        this.setConnection("live");
      }
      return;
    }
    if (live) this.setConnection("error", issueMessage(result.error.issues));
    console.error("dashboard: bad frame", result.error.issues);
  }

  private reconnect(message: string): void {
    if (this.stopped || this.reconnectTimer !== undefined) {
      return;
    }
    this.generation += 1;
    const previous = this.socket;
    this.socket = undefined;
    previous?.close();
    const phase = this.attempts >= OFFLINE_AFTER_ATTEMPTS ? "offline" : "reconnecting";
    this.setOutagePhase(phase, message);
    const delay = BACKOFF_MS[Math.min(this.attempts, BACKOFF_MS.length - 1)];
    this.attempts += 1;
    this.reconnectTimer = this.schedule(() => {
      this.reconnectTimer = undefined;
      this.openSocket();
    }, delay);
  }

  // An outage - "reconnecting" then "offline" - is one event with one
  // `since`, stamped when the feed first dropped: preserve it across
  // retries and across the reconnecting -> offline flip, and only stamp a
  // fresh `since` when this failure is what starts the outage.
  private setOutagePhase(phase: "reconnecting" | "offline", message: string): void {
    const current = this.store.get(connectionAtom);
    const inOutage = current.phase === "reconnecting" || current.phase === "offline";
    this.store.set(connectionAtom, {
      phase,
      since: inOutage ? current.since : Date.now(),
      message,
    });
  }
}

/** Connect the dashboard to the status snapshot endpoint and live socket. */
export function connectStatusSocket(store: Store, deps: SocketDeps = {}): () => void {
  const connection = new StatusSocketConnection(store, deps);
  connection.start();
  return () => connection.dispose();
}
