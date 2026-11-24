import type { EmulatorSize } from "@/components/terminal/emulator";

export type TerminalMode = "view" | "control";
export type TerminalPhase = "connecting" | "waiting" | "live" | "ended" | "dropped" | "refused";

export interface TerminalState {
  phase: TerminalPhase;
  mode: TerminalMode;
  reason?: string;
  /** Time when the current phase began, not when the latest frame arrived. */
  since: number;
}

export interface TerminalDeps {
  WebSocket?: typeof WebSocket;
  location?: { protocol: string; host: string };
  setTimeout?: typeof setTimeout;
  clearTimeout?: typeof clearTimeout;
  now?: () => number;
}

export const RETRY_WAITING_MS = 2000;
export const RETRY_DROPPED_MS = 1000;
/** The server sends `{"ready":true}` once tmux is attached; past this the attempt counts as dropped. */
export const STARTUP_TIMEOUT_MS = 10000;

/**
 * Below the server's 64 KiB `max_frame_size` (`bridge.rs`), leaving headroom
 * so a single frame never brushes the cap.
 */
const MAX_SEND_CHUNK_BYTES = 32 * 1024;

export class TerminalConnection {
  private attempts = 0;
  private openedOnce = false;
  private retryTimer: ReturnType<typeof setTimeout> | undefined;
  private startupTimer: ReturnType<typeof setTimeout> | undefined;
  private socket: WebSocket | undefined;
  private state: TerminalState | undefined;
  private stopped = false;
  private readonly Socket: typeof WebSocket;
  private readonly cancel: typeof clearTimeout;
  private readonly mode: TerminalMode;
  private readonly now: () => number;
  private readonly onState: (state: TerminalState) => void;
  private readonly schedule: typeof setTimeout;
  private readonly sink: (bytes: Uint8Array) => void;
  private readonly url: string;

  constructor(
    stageId: string,
    mode: TerminalMode,
    sink: (bytes: Uint8Array) => void,
    onState: (state: TerminalState) => void,
    deps: TerminalDeps = {},
  ) {
    this.mode = mode;
    this.sink = sink;
    this.onState = onState;
    this.Socket = deps.WebSocket ?? globalThis.WebSocket;
    this.schedule = deps.setTimeout ?? globalThis.setTimeout.bind(globalThis);
    this.cancel = deps.clearTimeout ?? globalThis.clearTimeout.bind(globalThis);
    this.now = deps.now ?? Date.now;
    const location = deps.location ?? globalThis.location;
    const scheme = location.protocol === "https:" ? "wss" : "ws";
    this.url = `${scheme}://${location.host}/ws/terminal/${encodeURIComponent(stageId)}/${mode}`;
    this.openSocket();
  }

  send(data: string): void {
    if (this.mode !== "control" || !this.socketIsOpen()) return;
    // Encode first, then slice the bytes: slicing the string instead and
    // encoding each piece separately could split a surrogate pair across two
    // pieces, corrupting it. The PTY reassembles a byte stream and does not
    // care where frame boundaries fall, so an oversized paste is chunked into
    // ordered frames on the same socket rather than sent as one frame that
    // would exceed the server's cap.
    const bytes = new TextEncoder().encode(data);
    for (let offset = 0; offset < bytes.length; offset += MAX_SEND_CHUNK_BYTES) {
      this.socket?.send(bytes.subarray(offset, offset + MAX_SEND_CHUNK_BYTES));
    }
  }

  resize(size: EmulatorSize): void {
    if (!this.socketIsOpen()) return;
    this.socket?.send(JSON.stringify({ resize: { cols: size.cols, rows: size.rows } }));
  }

  scroll(scroll: { pages: number }): void {
    if (!this.socketIsOpen()) return;
    this.socket?.send(JSON.stringify({ scroll }));
  }

  retry(): void {
    if (this.stopped) return;
    this.attempts = 0;
    this.cancelRetry();
    this.cancelStartup();
    const previous = this.socket;
    this.socket = undefined;
    previous?.close();
    this.openSocket();
  }

  close(): void {
    if (this.stopped) return;
    this.stopped = true;
    this.cancelRetry();
    this.cancelStartup();
    const socket = this.socket;
    this.socket = undefined;
    socket?.close();
  }

  private openSocket(): void {
    if (this.stopped) return;
    this.cancelStartup();
    this.emit("connecting");
    const socket = new this.Socket(this.url);
    this.socket = socket;
    socket.binaryType = "arraybuffer";
    socket.onopen = () => this.handleOpen(socket);
    socket.onmessage = (event) => this.handleMessage(socket, event.data);
    socket.onclose = (event) => this.handleClose(socket, event);
    socket.onerror = () => {
      if (this.socket !== socket) return;
    };
  }

  private handleOpen(socket: WebSocket): void {
    if (this.socket !== socket) return;
    // The handshake completed, which separates a refusal before any open from a
    // drop. The terminal is not live until the server's ready frame: the retry
    // budget resets there, so an attach that keeps failing stays bounded.
    this.openedOnce = true;
    this.startupTimer = this.schedule(() => this.startupExpired(socket), STARTUP_TIMEOUT_MS);
  }

  private startupExpired(socket: WebSocket): void {
    this.startupTimer = undefined;
    if (this.socket !== socket || this.stopped) return;
    this.socket = undefined;
    socket.close();
    this.dropped("terminal did not start");
  }

  private handleMessage(socket: WebSocket, data: unknown): void {
    if (this.socket !== socket) return;
    if (data instanceof ArrayBuffer) this.sink(new Uint8Array(data));
    else if (typeof data === "string" && isReadyFrame(data)) this.handleReady();
  }

  private handleReady(): void {
    if (this.state?.phase === "live") return;
    this.attempts = 0;
    this.openedOnce = true;
    this.cancelStartup();
    this.emit("live");
  }

  private handleClose(socket: WebSocket, event: CloseEvent): void {
    if (this.socket !== socket || this.stopped) return;
    this.cancelStartup();
    if (this.handleKnownClose(event)) return;
    this.dropped(event.reason || "connection lost");
  }

  private dropped(reason: string): void {
    if (!this.openedOnce) {
      this.emit("refused", "handshake refused");
    } else if (this.attempts === 0) {
      this.attempts = 1;
      this.emit("connecting", reason);
      this.scheduleRetry(RETRY_DROPPED_MS);
    } else {
      this.emit("dropped", reason);
    }
  }

  private handleKnownClose(event: CloseEvent): boolean {
    if (event.code === 1000) this.emit("ended", event.reason);
    else if (event.code === 1001) this.emit("dropped", "server stopping");
    // 1009 is the server's `CLOSE_TOO_LARGE`: a frame past its 64 KiB cap left
    // the input stream out of sync. Refuse automatic retry and let the user
    // reconnect deliberately after the server rejects the input.
    else if (event.code === 1009) this.emit("refused", "input too large");
    else if (event.code === 4004 || event.code === 4008 || event.code === 4011)
      this.emit("refused", event.reason);
    else if (event.code === 4009) {
      this.emit("waiting", event.reason);
      this.scheduleRetry(RETRY_WAITING_MS);
    } else return false;
    return true;
  }

  private scheduleRetry(delay: number): void {
    this.cancelRetry();
    this.retryTimer = this.schedule(() => {
      this.retryTimer = undefined;
      this.openSocket();
    }, delay);
  }

  private cancelStartup(): void {
    if (this.startupTimer === undefined) return;
    this.cancel(this.startupTimer);
    this.startupTimer = undefined;
  }

  private cancelRetry(): void {
    if (this.retryTimer === undefined) return;
    this.cancel(this.retryTimer);
    this.retryTimer = undefined;
  }

  private socketIsOpen(): boolean {
    return this.socket?.readyState === this.Socket.OPEN;
  }

  private emit(phase: TerminalPhase, reason?: string): void {
    const since = this.state?.phase === phase ? this.state.since : this.now();
    this.state = { phase, mode: this.mode, since, ...(reason ? { reason } : {}) };
    this.onState(this.state);
  }
}

function isReadyFrame(text: string): boolean {
  try {
    const value: unknown = JSON.parse(text);
    return typeof value === "object" && value !== null && Reflect.get(value, "ready") === true;
  } catch {
    return false;
  }
}
