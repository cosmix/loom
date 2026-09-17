import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { TerminalView } from "./terminal-view";
import type { Emulator, EmulatorFactory } from "./use-terminal";
import { TooltipProvider } from "@/components/ui/tooltip";
import { applySnapshot } from "@/state/apply";
import { snapshotAtom } from "@/state/atoms";
import {
  FakeSocket,
  fakeTimers,
  fakeEmulator,
  fakeFactory,
  terminalStage,
  snapshotFor,
  renderView,
  renderModal,
  well,
  waitForMount,
  live,
} from "./terminal-view-test-helpers";

afterEach(() => {
  cleanup();
  FakeSocket.instances = [];
  vi.unstubAllGlobals();
});

describe("terminal view", () => {
  it("opens in view mode and says viewing once the connection opens", async () => {
    const { emulator } = await live();
    expect(emulator.calls.readOnly).toEqual([true]);
    expect(await screen.findByText("viewing")).toBeTruthy();
    const socket = FakeSocket.instances[0];
    socket.sent.length = 0;
    emulator.scroll(-1);
    emulator.emit("should stay blocked");
    emulator.scroll(1);
    expect(socket.sent).toEqual(['{"scroll":{"pages":-1}}', '{"scroll":{"pages":1}}']);
    expect(well().dataset.mode).toBe("view");
  });

  it("returns to details when Esc is pressed in view mode", async () => {
    const timers = fakeTimers();
    const factory = fakeFactory();
    const stage = terminalStage();
    const { router } = renderModal(
      stage,
      factory.factory,
      timers.deps,
      `/?stage=${stage.id}&view=terminal`,
    );

    await waitForMount(factory);
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });

    await waitFor(() => expect(router.state.location.search).toBe(`?stage=${stage.id}`));
  });

  it("requires a double-click on the terminal to take control in view mode", async () => {
    const timers = fakeTimers();
    const factory = fakeFactory();
    renderView(terminalStage(), factory.factory, timers.deps);

    const first = await waitForMount(factory);
    act(() => FakeSocket.instances[0].open());
    await waitFor(() => expect(first.calls.readOnly).toEqual([true]));
    fireEvent.click(well());
    expect(well().dataset.mode).toBe("view");
    expect(factory.doubles).toHaveLength(1);
    expect(screen.getByText("double-click to take control")).toBeTruthy();
    fireEvent.doubleClick(well());
    await waitFor(() => expect(factory.doubles).toHaveLength(2));
    act(() => FakeSocket.instances[1].open());

    expect(factory.doubles[1].calls.readOnly).toEqual([false]);
    expect(factory.doubles[1].calls.focused).toBe(1);
    expect(well().dataset.mode).toBe("control");
  });

  it("releases control when the key is pressed again", async () => {
    const { factory } = await live();
    const key = () => screen.getByRole("switch", { name: "Take control" });
    // The active state label plus the knob's own rider, in DOM order: the
    // rider carries whichever verb the knob currently shows, and only the
    // cell the knob has uncovered shows its state word.
    const shown = () =>
      Array.from(key().querySelectorAll('[data-active="true"], .terminal-key-rider'))
        .map((label) => label.textContent)
        .join("|");
    await waitFor(() => expect(shown()).toBe("viewing|take control"));
    expect(key().getAttribute("aria-checked")).toBe("false");

    fireEvent.click(key());
    await waitFor(() => expect(factory.doubles).toHaveLength(2));
    act(() => FakeSocket.instances[1].open());
    await waitFor(() => expect(key().getAttribute("aria-checked")).toBe("true"));
    expect(well().dataset.mode).toBe("control");
    await waitFor(() => expect(shown()).toBe("controlling|release"), { timeout: 2000 });

    fireEvent.click(key());
    await waitFor(() => expect(well().dataset.mode).toBe("view"));
    expect(key().getAttribute("aria-checked")).toBe("false");
  });

  it("swaps the key's words only after the slide", async () => {
    await live();
    const key = () => screen.getByRole("switch", { name: "Take control" });
    const rider = () => key().querySelector(".terminal-key-rider");
    const cells = () => key().querySelectorAll(".terminal-key-cell");
    const controllingLabel = () => cells()[0].querySelector('[data-kind="state"]');
    const viewingLabel = () => cells()[1].querySelector('[data-kind="state"]');

    await waitFor(() => expect(rider()?.textContent).toBe("take control"));
    fireEvent.click(key());

    expect(key().getAttribute("aria-checked")).toBe("true");
    expect(rider()?.textContent).toBe("take control");
    expect(viewingLabel()?.getAttribute("data-active")).toBe("false");

    await waitFor(() => expect(controllingLabel()?.getAttribute("data-active")).toBe("true"), {
      timeout: 2000,
    });
    expect(rider()?.textContent).toBe("take control");

    await waitFor(() => expect(rider()?.textContent).toBe("release"), { timeout: 2000 });
  });

  it("turns the key into a plain indicator once the session has ended", async () => {
    await live();
    act(() => FakeSocket.instances[0].closeFromServer(1000));

    const key = await screen.findByRole("switch", { name: "Take control" });
    await waitFor(() => expect(key.textContent).toBe("ended"));
    expect(key).toHaveProperty("disabled", true);
    expect(well().dataset.phase).toBe("ended");
  });

  it("keeps the terminal open when Esc is pressed in control mode", async () => {
    const timers = fakeTimers();
    const factory = fakeFactory();
    const stage = terminalStage();
    const { router } = renderModal(
      stage,
      factory.factory,
      timers.deps,
      `/?stage=${stage.id}&view=terminal`,
    );

    await waitForMount(factory);
    fireEvent.click(screen.getByRole("switch", { name: "Take control" }));
    await waitFor(() => expect(well().dataset.mode).toBe("control"));
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });

    expect(router.state.location.search).toBe(`?stage=${stage.id}&view=terminal`);
  });

  it("allows an outside pointer-down to close view mode but not control mode", async () => {
    const stage = terminalStage();
    const view = fakeFactory();
    const viewRouter = renderModal(
      stage,
      view.factory,
      fakeTimers().deps,
      `/?stage=${stage.id}&view=terminal`,
    ).router;
    await waitForMount(view);
    await new Promise((resolve) => window.setTimeout(resolve, 0));
    // Radix defers an outside pointer-down to the trailing click (so a drag
    // selection that ends inside the dialog does not dismiss it), so a real
    // outside dismissal needs both events, exactly like a browser click.
    fireEvent.pointerDown(document.body);
    fireEvent.click(document.body);
    await waitFor(() => expect(viewRouter.state.location.search).toBe(""));
    cleanup();

    const control = fakeFactory();
    const controlRouter = renderModal(
      stage,
      control.factory,
      fakeTimers().deps,
      `/?stage=${stage.id}&view=terminal`,
    ).router;
    await waitForMount(control);
    fireEvent.click(screen.getByRole("switch", { name: "Take control" }));
    await waitFor(() => expect(screen.getByRole("dialog").dataset.mode).toBe("control"));
    await new Promise((resolve) => window.setTimeout(resolve, 0));
    fireEvent.pointerDown(document.body);
    fireEvent.click(document.body);

    expect(controlRouter.state.location.search).toBe(`?stage=${stage.id}&view=terminal`);
  });

  it("tab in control mode is stopped before it reaches the dialog's focus trap", async () => {
    const timers = fakeTimers();
    const factory = fakeFactory();
    const stage = terminalStage();
    renderModal(stage, factory.factory, timers.deps, `/?stage=${stage.id}&view=terminal`);

    await waitForMount(factory);
    fireEvent.click(screen.getByRole("switch", { name: "Take control" }));
    await waitFor(() => expect(factory.doubles).toHaveLength(2));
    const emulator = factory.doubles[1];
    await waitFor(() => expect(emulator.calls.opened).toHaveLength(1));
    const socket = FakeSocket.instances[1];
    act(() => socket.open());
    // Radix's focus-trap/dismissable-layer listeners (like most of its
    // internals) are plain `document.addEventListener` calls, not React
    // props, so they sit above wherever React's own delegated listener is
    // attached. A spy on the dialog *node* would fire on ordinary native
    // bubbling regardless of `stopPropagation`, since that call only takes
    // effect once the event reaches React's listener; `document` is the
    // one place a stopped event provably never reaches.
    const documentKeyDown = vi.fn();
    document.addEventListener("keydown", documentKeyDown);
    const input = document.createElement("textarea");
    input.addEventListener("keydown", (event) => {
      if (event.key === "Tab") emulator.emit("\t");
    });
    emulator.calls.opened[0].append(input);
    input.focus();
    fireEvent.keyDown(input, { key: "Tab" });
    document.removeEventListener("keydown", documentKeyDown);

    expect(documentKeyDown).not.toHaveBeenCalled();
    // jsdom's TextEncoder builds its Uint8Array from a realm distinct from the
    // test's global Uint8Array, so `instanceof` never matches; ArrayBuffer.isView
    // is the realm-safe way to confirm this is a binary frame, not a JSON one.
    expect(socket.sent.some((frame) => ArrayBuffer.isView(frame))).toBe(true);
    expect(Array.from(socket.sent.at(-1) as Uint8Array)).toEqual([9]);
  });

  it("tab in view mode reaches the dialog's focus trap", async () => {
    const timers = fakeTimers();
    const factory = fakeFactory();
    const stage = terminalStage();
    renderModal(stage, factory.factory, timers.deps, `/?stage=${stage.id}&view=terminal`);

    const emulator = await waitForMount(factory);
    await waitFor(() => expect(emulator.calls.opened).toHaveLength(1));
    // The negative half of the assertion above: a handler that stopped
    // every Tab unconditionally would also pass the control-mode test, so
    // this pins that view mode (the default here — no "Take control" click)
    // must let Tab reach the trap.
    const documentKeyDown = vi.fn();
    document.addEventListener("keydown", documentKeyDown);
    const input = document.createElement("textarea");
    emulator.calls.opened[0].append(input);
    input.focus();
    fireEvent.keyDown(input, { key: "Tab" });
    document.removeEventListener("keydown", documentKeyDown);

    expect(documentKeyDown).toHaveBeenCalledTimes(1);
  });

  it("unmount before the emulator resolves disposes the late emulator without opening a socket", async () => {
    const timers = fakeTimers();
    const late = fakeEmulator();
    let resolve: ((value: Emulator) => void) | undefined;
    const factory: EmulatorFactory = () =>
      new Promise((complete) => {
        resolve = complete;
      });
    const view = renderView(terminalStage(), factory, timers.deps);

    await waitFor(() => expect(resolve).toBeDefined());
    view.unmount();
    resolve?.(late.emulator);
    await waitFor(() => expect(late.calls.disposed).toBe(1));

    expect(late.calls.opened).toEqual([]);
    expect(FakeSocket.instances).toEqual([]);
  });

  it("shows a refused renderer failure without scheduling a retry", async () => {
    const timers = fakeTimers();
    const factory: EmulatorFactory = async () => Promise.reject(new Error("unavailable"));
    renderView(terminalStage(), factory, timers.deps);

    expect((await screen.findByRole("alert")).textContent).toContain(
      "terminal renderer failed to load",
    );
    expect(timers.active()).toEqual([]);
    expect(FakeSocket.instances).toEqual([]);
  });

  it("cancels a pending fit debounce when the view unmounts", async () => {
    class TestResizeObserver {
      static instances: TestResizeObserver[] = [];
      readonly callback: ResizeObserverCallback;

      constructor(callback: ResizeObserverCallback) {
        this.callback = callback;
        TestResizeObserver.instances.push(this);
      }

      observe(): void {}
      disconnect(): void {}
      fire(): void {
        this.callback([], this as unknown as ResizeObserver);
      }
    }

    // `src/test/setup.ts` installs its own ResizeObserver stub via
    // `Object.defineProperty(window, "ResizeObserver", { value: ... })` with no
    // `configurable: true`, unlike the `matchMedia`/`getBBox` stubs beside it in
    // that same file. A non-configurable property can never be redefined to a
    // different value by any means (`vi.stubGlobal`, `Object.defineProperty`,
    // or `delete`), so this stub cannot be swapped for one that fires from
    // this file alone; see the accompanying report for the one-line fix.
    vi.stubGlobal("ResizeObserver", TestResizeObserver);
    const timers = fakeTimers();
    const factory = fakeFactory();
    const view = renderView(terminalStage(), factory.factory, timers.deps);
    const emulator = await waitForMount(factory);
    await waitFor(() => expect(emulator.calls.opened).toHaveLength(1));
    const fitsBefore = emulator.calls.fit;
    TestResizeObserver.instances[0].fire();
    const pending = timers.active()[0];
    view.unmount();
    pending.callback();

    expect(emulator.calls.fit).toBe(fitsBefore);
  });

  it.each([
    [4009, "waiting", "waiting for the session's tmux server", true],
    [1000, "ended", "ended", false],
    [4004, "refused", "server reason", false],
    [4008, "refused", "server reason", false],
  ])("renders close code %i as %s", async (code, phase, text, retries) => {
    const { timers } = await live();
    act(() => FakeSocket.instances[0].closeFromServer(code));

    expect(await screen.findByText(text)).toBeTruthy();
    expect(well().dataset.phase).toBe(phase);
    expect(timers.active().length > 0).toBe(retries);
  });

  it("offers Retry after a second dropped connection and opens a new socket", async () => {
    const { timers } = await live();
    act(() => FakeSocket.instances[0].closeFromServer(1006, "first drop"));
    timers.fireNext();
    act(() => FakeSocket.instances[1].closeFromServer(1006, "second drop"));

    fireEvent.click(await screen.findByRole("button", { name: "Retry" }));
    expect(FakeSocket.instances).toHaveLength(3);
  });

  it.each([
    [false, "Start the dashboard with loom status --web --terminals"],
    [true, "Dashboard cookie missing — open the tokenized URL printed when the dashboard started"],
  ])("explains a pre-open refusal when terminals are %s", async (terminals, message) => {
    const timers = fakeTimers();
    const factory = fakeFactory();
    const store = createStore();
    applySnapshot(store, snapshotFor(terminalStage(), terminals));
    render(
      <Provider store={store}>
        <TooltipProvider>
          <TerminalView
            stage={terminalStage()}
            frame="page"
            factory={factory.factory}
            deps={timers.deps}
          />
        </TooltipProvider>
      </Provider>,
    );
    await waitForMount(factory);
    act(() => FakeSocket.instances[0].closeFromServer(1006));

    expect((await screen.findByRole("alert")).textContent).toContain(message);
  });

  it("offers Reconnect when an ended stage becomes alive again", async () => {
    const stage = terminalStage({ session_alive: false });
    const { store } = await live(stage);
    act(() => FakeSocket.instances[0].closeFromServer(1000));
    act(() => store.set(snapshotAtom, snapshotFor({ ...stage, session_alive: true })));

    fireEvent.click(await screen.findByRole("button", { name: "Reconnect" }));
    expect(FakeSocket.instances).toHaveLength(2);
  });

  it("shows the attach command and encoded page link in a dialog footer", async () => {
    const timers = fakeTimers();
    const factory = fakeFactory();
    const stage = terminalStage({ id: "stage/a" });
    renderModal(stage, factory.factory, timers.deps, "/?stage=stage%2Fa&view=terminal");
    await waitForMount(factory);

    expect(screen.getByText("loom attach stage/a")).toBeTruthy();
    expect(screen.getByRole("link", { name: /open in a tab/ }).getAttribute("href")).toBe(
      "/terminal/stage%2Fa",
    );
  });
});
