import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { Presence } from "@/plan/header/presence";
import { authoringAtom, presenceAtom, youAtom } from "@/plan/state/atoms";

afterEach(cleanup);
beforeEach(() => window.localStorage.setItem("loom-plan-name", "Alex"));

function entry(id: string, name: string, extra: Record<string, unknown> = {}) {
  return { public_id: id, name, holder: false, away: false, reading: null, tabs: 1, ...extra };
}

function openPresence(entries: ReturnType<typeof entry>[], holderFrees: string | null = null) {
  const store = createStore();
  store.set(youAtom, { public_id: "me", name: "Alex" });
  store.set(presenceAtom, { entries });
  const holder = entries.find((candidate) => candidate.holder);
  store.set(authoringAtom, {
    holder: holder
      ? { public_id: holder.public_id, name: holder.name, away_since: null, frees_at: holderFrees }
      : null,
  });
  render(
    <Provider store={store}>
      <Presence />
    </Provider>,
  );
  fireEvent.click(screen.getByRole("button", { name: /people here/i }));
  return within(screen.getByRole("list", { name: "People here" }));
}

function renderPresence(entries: ReturnType<typeof entry>[]) {
  const store = createStore();
  store.set(youAtom, { public_id: "me", name: "Alex" });
  store.set(presenceAtom, { entries });
  render(
    <Provider store={store}>
      <Presence />
    </Provider>,
  );
  return screen.getByRole("button", { name: /people here/i });
}

describe("Presence", () => {
  it("shows initials avatars for at most three people, then a +N circle", () => {
    const trigger = renderPresence([
      entry("me", "Alex"),
      entry("ada", "Ada Lovelace"),
      entry("bob", "bob"),
      entry("cy", "Cy"),
      entry("di", "Di"),
    ]);
    expect(trigger.getAttribute("aria-label")).toBe("People here: 5");
    const circles = Array.from(trigger.querySelectorAll(".plan-avatar"));
    expect(circles.map((circle) => circle.textContent)).toEqual(["A", "AL", "B", "+2"]);
  });

  it("gives the same person the same hue and rings the authoring holder first", () => {
    const trigger = renderPresence([entry("me", "Alex"), entry("ada", "Ada", { holder: true })]);
    const [first, second] = Array.from(trigger.querySelectorAll<HTMLElement>(".plan-avatar"));
    expect(first!.textContent).toBe("A");
    expect(first!.hasAttribute("data-holder")).toBe(true);
    expect(second!.hasAttribute("data-holder")).toBe(false);
    expect(first!.style.getPropertyValue("--avatar-hue")).not.toBe(
      second!.style.getPropertyValue("--avatar-hue"),
    );
    cleanup();
    const again = renderPresence([entry("ada", "Ada")]);
    expect(
      again.querySelector<HTMLElement>(".plan-avatar")!.style.getPropertyValue("--avatar-hue"),
    ).toBe(first!.style.getPropertyValue("--avatar-hue"));
  });

  it("takes initials from letters and digits only", () => {
    const trigger = renderPresence([entry("eve", "<img src=x>")]);
    expect(trigger.querySelector(".plan-avatar")!.textContent).toBe("IS");
    expect(trigger.querySelector("img")).toBeNull();
  });

  it("marks you, the holder and an away holder with the free-up time", () => {
    const list = openPresence(
      [
        entry("me", "Alex"),
        entry("ada", "Ada", { holder: true, away: true, reading: "doc/plans/a.md" }),
      ],
      "2026-10-27T10:10:00Z",
    );
    const [me, ada] = list.getAllByRole("listitem");
    expect(me!.textContent).toContain("(you)");
    expect(me!.textContent).not.toContain("authoring");
    expect(ada!.textContent).toContain("authoring");
    expect(ada!.textContent).toMatch(/away, frees at /);
    expect(ada!.textContent).toContain("doc/plans/a.md");
  });

  it("names its dialog and keeps the rows out of the tab order", () => {
    const list = openPresence([entry("me", "Alex"), entry("ada", "Ada", { reading: "a.md" })]);
    expect(screen.getByRole("dialog", { name: "People here" })).toBeTruthy();
    for (const row of list.getAllByRole("listitem")) {
      expect(row.hasAttribute("tabindex")).toBe(false);
    }
  });

  it("renders a hostile name as literal text", () => {
    const hostile = "<img src=x onerror=alert(1)>";
    const list = openPresence([entry("eve", hostile)]);
    expect(list.getByText(hostile)).toBeTruthy();
    expect(document.querySelector("img")).toBeNull();
  });

  it("shows a name, a reading path and a free-up fallback with their controls spelled out", () => {
    const list = openPresence(
      [entry("eve", "Eve\u202E", { holder: true, away: true, reading: "doc/\u202Ea.md" })],
      "soon\u202E",
    );
    const text = list.getByRole("listitem").textContent;
    expect(text).toContain("Eve⟨U+202E⟩");
    expect(text).toContain("doc/⟨U+202E⟩a.md");
    expect(text).toContain("frees at soon⟨U+202E⟩");
    expect(text).not.toContain("\u202E");
  });

  it("asks for a name only while none is stored", () => {
    window.localStorage.removeItem("loom-plan-name");
    openPresence([entry("me", "Alex")]);
    expect(screen.getByLabelText("Your name")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Your name"), { target: { value: "Alexandra" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(window.localStorage.getItem("loom-plan-name")).toBe("Alexandra");
    expect(screen.queryByLabelText("Your name")).toBeNull();
  });

  it("does not ask when a name is stored", () => {
    openPresence([entry("me", "Alex")]);
    expect(screen.queryByLabelText("Your name")).toBeNull();
  });
});
