import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import type { HistoryLoader } from "@/api/terminal-history";
import { HistoryView } from "@/components/terminal/history-view";
import { fakeFactory, terminalStage } from "@/components/terminal/terminal-view-test-helpers";
import { TooltipProvider } from "@/components/ui/tooltip";

const SESSION = "session-ab12cd34-1700000000";

function renderHistory(load: HistoryLoader) {
  const emulators = fakeFactory();
  const stage = terminalStage({ session_alive: false, terminal_history_session: SESSION });
  const router = createMemoryRouter([
    {
      path: "/",
      element: (
        <TooltipProvider>
          <HistoryView
            stage={stage}
            session={SESSION}
            frame="page"
            factory={emulators.factory}
            load={load}
          />
        </TooltipProvider>
      ),
    },
  ]);
  const view = render(<RouterProvider router={router} />);
  return { ...emulators, view };
}

afterEach(cleanup);

describe("HistoryView", () => {
  it("replays the saved bytes into a read-only terminal", async () => {
    const saved = new TextEncoder().encode("\u001b[31mred\u001b[0m\nline two\n");
    const { doubles } = renderHistory(async () => saved);

    await waitFor(() => expect(doubles[0]?.calls.written).toHaveLength(1));

    expect(doubles[0].calls.written[0]).toEqual(saved);
    expect(doubles[0].calls.readOnly).toEqual([true]);
    expect(doubles[0].calls.opened).toEqual([screen.getByLabelText("Saved terminal history")]);
    expect(screen.getByText("Terminal history")).toBeTruthy();
    expect(screen.getByText(/read-only/)).toBeTruthy();
  });

  it("asks for the history of the session it was given", async () => {
    const asked: string[] = [];
    const { doubles } = renderHistory(async (session) => {
      asked.push(session);
      return new Uint8Array([104, 105]);
    });

    await waitFor(() => expect(doubles[0]?.calls.written).toHaveLength(1));

    expect(asked).toEqual([SESSION]);
  });

  it("offers the less command that reads the same file", async () => {
    const { doubles } = renderHistory(async () => new Uint8Array([104]));
    await waitFor(() => expect(doubles[0]?.calls.written).toHaveLength(1));

    expect(
      screen.getByRole("button", {
        name: `copy less -R .loom/work/terminal-history/${SESSION}.terminal`,
      }),
    ).toBeTruthy();
  });

  it("says so when the session left no history, and keeps no terminal", async () => {
    const { doubles } = renderHistory(async () => null);

    expect((await screen.findByRole("alert")).textContent).toContain("left no saved terminal");
    expect(doubles[0].calls.opened).toHaveLength(0);
    expect(doubles[0].calls.disposed).toBe(1);
  });

  it("says so when the history cannot be loaded", async () => {
    const { doubles } = renderHistory(async () => {
      throw new Error("HTTP 500");
    });

    expect((await screen.findByRole("alert")).textContent).toContain("could not be loaded");
    expect(doubles[0].calls.disposed).toBe(1);
  });

  it("disposes the terminal when it unmounts", async () => {
    const { doubles, view } = renderHistory(async () => new Uint8Array([104]));
    await waitFor(() => expect(doubles[0]?.calls.written).toHaveLength(1));

    view.unmount();

    expect(doubles[0].calls.disposed).toBe(1);
  });
});
