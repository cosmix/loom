import { parseFrame } from "@/api/frames";
import { applyFrame } from "@/state/apply";
import { connectionAtom, type ConnectionPhase } from "@/state/atoms";
import type { Store } from "@/state/store";

export interface SocketDeps {
  WebSocket?: typeof WebSocket;
  location?: { protocol: string; host: string };
  setTimeout?: typeof setTimeout;
  clearTimeout?: typeof clearTimeout;
  setInterval?: typeof setInterval;
  clearInterval?: typeof clearInterval;
  now?: () => number;
}

// The server sends a `tick` on every collect (every 1-2s, loom feed.rs), so 30s
// without any message means the socket is dead even though no close event fired
// (a half-open TCP connection never fires one).
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
  private readonly schedule: typeof setTimeout;
  private readonly cancel: typeof clearTimeout;
  private watchdog: ReturnType<typeof setInterval> | undefined;
  private lastMessageAt = 0;
  private attemptStartedAt = 0;
  private opened = false;
  // Set by the socket's first `full`; deltas before it have no snapshot to change.
  private primed = false;
  private readonly every: typeof setInterval;
  private readonly stopEvery: typeof clearInterval;
  private readonly now: () => number;
  private readonly Socket: typeof WebSocket;
  private readonly url: string;
  private readonly store: Store;

  constructor(store: Store, deps: SocketDeps) {
    this.store = store;
    this.Socket = deps.WebSocket ?? globalThis.WebSocket;
    // Bound: an unbound `globalThis.setTimeout`/`clearTimeout` called as
    // `this.schedule(...)` etc. runs with `this` set to the client instance
    // rather than `globalThis`, which native implementations reject with
    // "Illegal invocation" in a real browser (only mocked in jsdom/tests).
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
  }

  dispose(): void {
    this.stopped = true;
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
    if (!this.opened) {
      if (this.now() - this.attemptStartedAt >= STALE_AFTER_MS) {
        this.reconnect("connection attempt timed out");
      }
      return;
    }
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
    this.attemptStartedAt = this.now();
    this.opened = false;
    this.primed = false;
    const socket = new this.Socket(this.url);
    this.socket = socket;
    // Every handler checks it's still the current socket: a superseded
    // socket can fire onclose/onerror after a later reconnect has already
    // opened a fresh one, and reacting to that stale event would schedule a
    // second, untracked reconnect loop.
    socket.onopen = () => {
      if (this.stopped || this.socket !== socket) return;
      this.opened = true;
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
    let value: unknown;
    try {
      value = JSON.parse(String(data));
    } catch (error) {
      this.primed = false;
      this.setConnection("error", "invalid JSON frame");
      console.error("dashboard: bad frame", error);
      return;
    }
    const result = parseFrame(value);
    if (!result.success) {
      // A rejected frame leaves the snapshot behind the server, so later deltas
      // would apply to stale data and a valid tick would hide the error. Drop
      // deltas until a `full` parses.
      this.primed = false;
      this.setConnection("error", issueMessage(result.issues));
      console.error("dashboard: bad frame", result.issues);
      return;
    }
    const frame = result.data;
    if (frame.t === "full") {
      this.primed = true;
    } else if (!this.primed) {
      // A socket always opens with a `full`; a delta first means the stream
      // is out of step, so it is dropped rather than applied to old data.
      console.error("dashboard: frame before the first full snapshot", frame.t);
      return;
    }
    applyFrame(this.store, frame);
    // Only transition on phase change: `since` marks when the phase began,
    // not when the last frame arrived (header-lines.tsx relies on that
    // distinction), so calling setConnection on every frame would stamp a
    // new `since` and break it. This also clears a stale error `message`
    // once the feed recovers.
    if (this.store.get(connectionAtom).phase !== "live") {
      this.setConnection("live");
    }
  }

  private reconnect(message: string): void {
    if (this.stopped || this.reconnectTimer !== undefined) {
      return;
    }
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

/** Connect the dashboard to the live status socket. */
export function connectStatusSocket(store: Store, deps: SocketDeps = {}): () => void {
  const connection = new StatusSocketConnection(store, deps);
  connection.start();
  return () => connection.dispose();
}
