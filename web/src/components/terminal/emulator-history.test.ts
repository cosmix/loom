import { beforeEach, describe, expect, it, vi } from "vitest";

import { createHistoryEmulator, createXtermEmulator } from "./emulator";

const created = vi.hoisted(() => ({ options: [] as Record<string, unknown>[] }));

vi.mock("./fonts", () => ({
  TERMINAL_FONT_FAMILY: "monospace",
  loadTerminalFonts: async () => undefined,
}));
vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    unicode = { activeVersion: "" };
    constructor(options: Record<string, unknown>) {
      created.options.push(options);
    }
    loadAddon() {}
    attachCustomKeyEventHandler() {}
  },
}));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: class {} }));
vi.mock("@xterm/addon-unicode11", () => ({ Unicode11Addon: class {} }));
vi.mock("@xterm/addon-webgl", () => ({ WebglAddon: class {} }));

const RANGE = { start: { x: 1, y: 1 }, end: { x: 5, y: 1 } };

describe("terminal emulator links", () => {
  beforeEach(() => {
    created.options.length = 0;
    vi.restoreAllMocks();
  });

  it("gives a saved screen a link handler that follows nothing", async () => {
    const open = vi.spyOn(window, "open").mockReturnValue(null);
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);

    await createHistoryEmulator();

    const handler = created.options[0]?.linkHandler as {
      activate(event: MouseEvent, text: string, range: typeof RANGE): void;
      allowNonHttpProtocols?: boolean;
    };
    expect(handler).toBeDefined();
    handler.activate(new MouseEvent("click"), "https://example.com/page", RANGE);
    handler.activate(new MouseEvent("click"), "file:///etc/passwd", RANGE);
    expect(open).not.toHaveBeenCalled();
    expect(confirm).not.toHaveBeenCalled();
    expect(handler.allowNonHttpProtocols).toBeFalsy();
  });

  it("leaves a live terminal on xterm's own link handling", async () => {
    await createXtermEmulator();

    expect(created.options[0]).not.toHaveProperty("linkHandler");
  });
});
