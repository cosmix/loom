import { cleanup, render, within } from "@testing-library/react";
import { Provider } from "jotai";
import { afterEach, expect, it, vi } from "vitest";

import { StateLine } from "./state-line";
import { fixture, item, mount, turn } from "./transcript-test-kit";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const tick = "`".repeat(3);
const fence = JSON.stringify({ decisions: ["Keep it small"], questions: ["Who owns it?"] });
const said = item("user", {
  id: "u1",
  text: "Write it",
  origin: null,
  attachments: [],
  delivery: "delivered",
});
const reply = item("text", {
  id: "done",
  markdown: `All written.\n\n${tick}loom-done\n${fence}\n${tick}\n`,
});

function ended(state: "ended" | "ending") {
  const talk = turn([said, reply], { open: false });
  return mount([], { session: { ...fixture.session!, state }, turns: [talk] });
}

function stateLine(store: ReturnType<typeof mount>["store"]) {
  return render(
    <Provider store={store}>
      <StateLine />
    </Provider>,
  );
}

it("an agent-ended chat shows the planner-finished card from the fence and never the raw fence", () => {
  const h = ended("ended");

  const card = h.log.querySelector(".chat-left-behind")!;
  expect(card.querySelector("h3")?.textContent).toBe("The planner finished this chat");
  expect(card.textContent).toContain("Keep it small");
  expect(card.textContent).toContain("Who owns it?");
  expect(h.log.querySelector(".chat-wrap")).toBeNull();
  expect(h.log.textContent).toContain("All written.");
  expect(h.log.textContent).not.toContain("loom-done");
  expect(h.log.textContent).not.toContain(fence);

  expect(stateLine(h.store).container.textContent).toContain("Finished by the planner");
});

it("an agent-ended chat that is still ending draws no wrap-up card and promises no closing write", () => {
  const h = ended("ending");

  expect(h.log.querySelector(".chat-wrap")).toBeNull();
  expect(within(h.log).queryByRole("heading", { name: "Writing down what we decided" })).toBeNull();
  expect(within(h.log).queryByText(/final notes/i)).toBeNull();
  expect(h.log.textContent).not.toContain("loom-done");

  const line = stateLine(h.store);
  expect(line.container.textContent).not.toContain("Writing final notes");
  expect(line.getAllByRole("status")[0]!.textContent).toBe("Ending the chat");
});

it("a user-written chat that is ending without a fence still draws the wrap-up card", () => {
  const h = mount([], {
    session: { ...fixture.session!, state: "ending" },
    turns: [turn([said, item("text", { id: "plain", markdown: "Done." })], { open: false })],
  });
  expect(h.log.querySelector(".chat-wrap")).not.toBeNull();
  expect(stateLine(h.store).getAllByRole("status")[0]!.textContent).toContain(
    "Writing final notes",
  );
});
