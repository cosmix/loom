import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Provider } from "jotai/react";
import { createStore } from "jotai/vanilla";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Overflow } from "@/plan/header/overflow";

afterEach(cleanup);

function show() {
  const onOpenSettings = vi.fn();
  const onOpenHelp = vi.fn();
  render(
    <Provider store={createStore()}>
      <MemoryRouter initialEntries={["/"]}>
        <Overflow onOpenSettings={onOpenSettings} onOpenHelp={onOpenHelp} />
      </MemoryRouter>
    </Provider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "More" }));
  return { onOpenSettings, onOpenHelp };
}

describe("Overflow", () => {
  it("holds the view switch, presence, Share, theme, Settings and Help", async () => {
    show();

    const menu = await screen.findByRole("dialog", { name: "More" });
    expect(within(menu).getByRole("navigation", { name: "Views" })).toBeTruthy();
    expect(within(menu).getByRole("button", { name: /^People here/ })).toBeTruthy();
    expect(within(menu).getByRole("button", { name: "Share" })).toBeTruthy();
    expect(within(menu).getByRole("button", { name: /^Switch to/ })).toBeTruthy();
    expect(within(menu).getByRole("button", { name: "Settings" }).textContent).toBe("Settings");
    expect(within(menu).getByRole("button", { name: "Help" }).textContent).toBe("Help");
  });

  it.each([
    ["Settings", "onOpenSettings"],
    ["Help", "onOpenHelp"],
  ] as const)("closes before %s opens its dialog", async (name, callback) => {
    const actions = show();
    const menu = await screen.findByRole("dialog", { name: "More" });

    fireEvent.click(within(menu).getByRole("button", { name }));

    expect(actions[callback]).toHaveBeenCalledOnce();
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "More" })).toBeNull());
  });
});
