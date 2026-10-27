import { cleanup, render } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, describe, expect, it } from "vitest";

import { connectionAtom } from "@/plan/state/atoms";
import { Unauthorized } from "@/plan/unauthorized";

afterEach(cleanup);

function text(everOpen: boolean): string {
  const store = createStore();
  store.set(connectionAtom, { phase: "unauthorized", lastMessageAt: null, everOpen });
  const { container } = render(
    <Provider store={store}>
      <Unauthorized />
    </Provider>,
  );
  return (container.textContent ?? "").replace(/\s+/g, " ");
}

describe("Unauthorized", () => {
  it("shows only the section 12 wording on a first visit", () => {
    const shown = text(false);
    expect(shown).toContain(
      "Open the link loom plan printed. Running loom plan again in this project prints it.",
    );
    expect(shown).not.toContain("restarted");
  });

  it("leads with the restart sentence when the page was connected", () => {
    const shown = text(true);
    expect(shown).toContain("loom plan restarted. Open the URL it printed in the terminal.");
    expect(shown.indexOf("restarted")).toBeLessThan(shown.indexOf("Open the link"));
    expect(shown).toContain("Running loom plan again in this project prints it.");
  });
});
