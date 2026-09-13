import { loadTerminalFonts, TERMINAL_FONT_FAMILY } from "./fonts";
import { handleTerminalKey } from "./keyboard";

export interface EmulatorSize {
  cols: number;
  rows: number;
}

export interface Emulator {
  open(host: HTMLElement): void;
  write(data: Uint8Array): void;
  /** Keystrokes from the user (already encoded by xterm). */
  onData(handler: (data: string) => void): () => void;
  /** Recompute cols/rows from the host box; null before open. */
  fit(): EmulatorSize | null;
  focus(): void;
  /** view = stdin disabled, hollow bar cursor, no blink; control = block cursor, blink. */
  setReadOnly(readOnly: boolean): void;
  dispose(): void;
}

export type EmulatorFactory = () => Promise<Emulator>;

interface TerminalTheme {
  background: string;
  foreground: string;
  cursor: string;
  selectionBackground: string;
  black: string;
  red: string;
  green: string;
  yellow: string;
  blue: string;
  magenta: string;
  cyan: string;
  white: string;
  brightBlack: string;
  brightRed: string;
  brightGreen: string;
  brightYellow: string;
  brightBlue: string;
  brightMagenta: string;
  brightCyan: string;
  brightWhite: string;
}

// The well's near-black ground in each mode. Keep these in sync with the
// `--well` / `--well-control` custom properties in terminal.css: the WebGL
// renderer paints `theme.background` opaquely on its own canvas, so the CSS
// and JS values must match exactly or the canvas and the well's padding
// meet with a visible seam.
const WELL = "#0a111f";
const WELL_CONTROL = "#240f0f";

/** The dark-well palette xterm needs as concrete colours. */
export function terminalTheme(): TerminalTheme {
  // ANSI accents mirror dashboard tones: blue=executing, green=completed,
  // red=blocked, yellow=warning, and cyan=queued. The cursor and selection take
  // the dashboard's yellow accent (the dark theme's --primary).
  return {
    background: WELL,
    foreground: "#d8dce3",
    cursor: "#f4d660",
    selectionBackground: "#f4d66040",
    black: WELL,
    red: "#e07a6f",
    green: "#7fc79a",
    yellow: "#d9b96a",
    blue: "#8fb4f0",
    magenta: "#c59bd9",
    cyan: "#7fc3cf",
    white: "#c8cbd3",
    brightBlack: "#89919e",
    brightRed: "#ed978e",
    brightGreen: "#9bd8ae",
    brightYellow: "#e6cc88",
    brightBlue: "#acc8f3",
    brightMagenta: "#d8b6e5",
    brightCyan: "#9bd6df",
    brightWhite: "#eef0f4",
  };
}

function terminalOptions() {
  return {
    allowProposedApi: true,
    cursorStyle: "bar" as const,
    fontFamily: TERMINAL_FONT_FAMILY,
    fontSize: 13,
    lineHeight: 1.4,
    // Option+B/F and other readline shortcuts must emit Meta sequences on macOS.
    macOptionIsMeta: true,
    scrollback: 2000,
    theme: terminalTheme(),
  };
}

function readOnlyOptions(readOnly: boolean) {
  return {
    disableStdin: readOnly,
    cursorBlink: !readOnly,
    cursorStyle: readOnly ? ("bar" as const) : ("block" as const),
    cursorInactiveStyle: "outline" as const,
    // Control mode tints the canvas itself, since the WebGL renderer ignores
    // the CSS background behind it.
    theme: { ...terminalTheme(), background: readOnly ? WELL : WELL_CONTROL },
  };
}

function installWebgl<T extends { dispose(): void; onContextLoss(handler: () => void): unknown }>(
  create: () => T,
  load: (addon: T) => void,
): void {
  let addon: T | undefined;
  try {
    const activeAddon = create();
    addon = activeAddon;
    activeAddon.onContextLoss(() => activeAddon.dispose());
    load(activeAddon);
  } catch {
    addon?.dispose();
    // xterm keeps its DOM renderer when WebGL is unavailable.
  }
}

/** Lazily imports @xterm packages so the main bundle does not carry them. */
export const createXtermEmulator: EmulatorFactory = async () => {
  const [{ Terminal }, { FitAddon }, { WebglAddon }, { Unicode11Addon }] = await Promise.all([
    import("@xterm/xterm"),
    import("@xterm/addon-fit"),
    import("@xterm/addon-webgl"),
    import("@xterm/addon-unicode11"),
    loadTerminalFonts(),
  ]);
  const terminal = new Terminal(terminalOptions());
  terminal.attachCustomKeyEventHandler((event) => handleTerminalKey(event, terminal));
  const fitAddon = new FitAddon();
  const unicodeAddon = new Unicode11Addon();
  terminal.loadAddon(unicodeAddon);
  terminal.unicode.activeVersion = "11";
  terminal.loadAddon(fitAddon);
  let opened = false;

  return {
    open(host) {
      terminal.open(host);
      opened = true;
      installWebgl(
        () => new WebglAddon(),
        (addon) => terminal.loadAddon(addon),
      );
    },
    write(data) {
      terminal.write(data);
    },
    onData(handler) {
      const subscription = terminal.onData(handler);
      return () => subscription.dispose();
    },
    fit() {
      if (!opened) return null;
      fitAddon.fit();
      return { cols: terminal.cols, rows: terminal.rows };
    },
    focus() {
      terminal.focus();
    },
    setReadOnly(readOnly) {
      terminal.options = readOnlyOptions(readOnly);
    },
    dispose() {
      terminal.dispose();
    },
  };
};
