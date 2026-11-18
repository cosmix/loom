import { fireEvent, render, screen } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { MemoryRouter } from "react-router";
import { beforeEach, expect, it, vi } from "vitest";

import fixture from "@/plan/api/fixtures/chat-snapshot.json";
import { chatSnapshotSchema } from "@/plan/api/schema-chat";
import composerCss from "@/plan/chat/chat-composer.css?raw";
import { ItemView } from "./items";
import { snapshotAtom } from "./model";
import { installPanelLifecycle } from "./panel-test-kit";
import { item, turn } from "./transcript-test-kit";

// Wide matchMedia, cleanup, mock restore and global unstub per test.
installPanelLifecycle();

beforeEach(() => {
  // The head and the opening label load the session brief; it never answers here.
  vi.stubGlobal(
    "fetch",
    vi.fn(() => new Promise<Response>(() => {})),
  );
});

const snapshot = chatSnapshotSchema.parse(fixture);

type Rule = { at: string[]; selectors: string[]; declarations: Array<[string, string]> };

/** Flat CSS rules with their enclosing at-rules; enough for the plain stylesheets read here. */
function cssRules(css: string): Rule[] {
  const text = css.replace(/\/\*[\s\S]*?\*\//g, "");
  const rules: Rule[] = [];
  const stack: string[] = [];
  let start = 0;
  for (let index = 0; index < text.length; index += 1) {
    const char = text[index];
    if (char === "}") {
      stack.pop();
      start = index + 1;
    } else if (char === "{") {
      const head = (text.slice(start, index).split(";").pop() ?? "").trim();
      if (head.startsWith("@")) {
        stack.push(head);
        start = index + 1;
        continue;
      }
      const end = text.indexOf("}", index);
      const declarations = text
        .slice(index + 1, end)
        .split(";")
        .map((entry) => entry.split(/:(.*)/s).map((part) => part.trim()))
        .filter((pair): pair is [string, string] => pair.length >= 2 && pair[0] !== "")
        .map(([property, value]) => [property.toLowerCase(), value] as [string, string]);
      rules.push({ at: [...stack], selectors: head.split(",").map((s) => s.trim()), declarations });
      index = end;
      start = end + 1;
    }
  }
  return rules;
}

it("an opening shows its label and hides the prompt until Show", () => {
  const prompt = "loom plan: no step chosen.\nAsk me what I want to work on.";
  const opening = item("user", { text: prompt, origin: "offer", attachments: [] });
  const store = createStore();
  store.set(snapshotAtom, {
    ...snapshot,
    previous: null,
    queue: [],
    session: { ...snapshot.session!, state: "working" },
    turns: [turn([opening])],
  });
  render(
    <Provider store={store}>
      <MemoryRouter>
        <ItemView item={opening} turn={turn([opening])} />
      </MemoryRouter>
    </Provider>,
  );
  expect(screen.getByText("Opening message")).toBeTruthy();
  expect(screen.queryByText(/loom plan: no step chosen\./)).toBeNull();
  const show = screen.getByRole("button", { name: "Show" });
  const controlled = show.getAttribute("aria-controls");
  expect(controlled).toBeTruthy();
  expect(document.getElementById(controlled!)).not.toBeNull();
  fireEvent.click(show);
  expect(screen.getByText(/loom plan: no step chosen\./)).toBeTruthy();
});

it("the composer has a visible border", () => {
  const box = cssRules(composerCss).filter((rule) => rule.selectors.includes(".chat-composer-box"));
  expect(box.length).toBeGreaterThan(0);
  const declarations = box.flatMap((rule) => rule.declarations);
  const borders = declarations.filter(([property]) => property === "border");
  expect(borders.some(([, value]) => /^1px\s+solid\s+var\(--[\w-]+\)$/.test(value))).toBe(true);
  expect(borders.some(([, value]) => /^(0|none)\b/.test(value))).toBe(false);
  expect(
    declarations.some(
      ([property, value]) => /^border(-width)?$/.test(property) && /^0(px)?$/.test(value),
    ),
  ).toBe(false);
  expect(
    declarations.some(
      ([property, value]) => property === "border-color" && value === "transparent",
    ),
  ).toBe(false);
});
