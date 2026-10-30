import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import hello from "@/plan/api/fixtures/hello.json";
import { serverInfoSchema } from "@/plan/api/schema";
import type { KickoffShape } from "@/plan/api/schema-kickoff";
import type { connectPlanSocket } from "@/plan/api/socket";
import { FEATURE_NAME_HINT } from "@/plan/new/feature-name";
import { Proposal } from "@/plan/new/proposal";
import {
  authoringAtom,
  connectionAtom,
  planSocketAtom,
  serverInfoAtom,
  youAtom,
} from "@/plan/state/atoms";

const featureShape: KickoffShape = {
  size: "feature",
  name: "planner-search",
  proposal: true,
  problem: "Finding plans takes too long.",
  decisions: [],
  questions: [],
  sections: [],
};
const clientId = "00000000-0000-4000-8000-000000000001";

function proposalStore() {
  const store = createStore();
  store.set(serverInfoAtom, { ...hello.server, read_only: [] });
  store.set(youAtom, hello.you);
  store.set(authoringAtom, { holder: { ...hello.you, away_since: null, frees_at: null } });
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: null });
  return store;
}

function mount(shape = featureShape, store = proposalStore()) {
  const listeners = new Map<string, Set<(message: unknown) => void>>();
  const send = vi.fn((_message: unknown) => true);
  const socket = {
    send,
    subscribe: (type: string, handler: (message: unknown) => void) => {
      const handlers = listeners.get(type) ?? new Set();
      listeners.set(type, handlers);
      handlers.add(handler);
      return () => {
        handlers.delete(handler);
      };
    },
    onResync: () => () => {},
    dispose: () => {},
  };
  store.set(planSocketAtom, socket as unknown as ReturnType<typeof connectPlanSocket>);
  const view = render(
    <Provider store={store}>
      <Proposal shape={shape} />
    </Provider>,
  );
  const result = async (error: string | null = null) => {
    await act(async () => {
      for (const handler of listeners.get("chat") ?? []) {
        handler({ type: "chat", frame: "result", client_id: clientId, ok: error === null, error });
      }
    });
  };
  return { ...view, store, send, result };
}

beforeEach(() => {
  vi.spyOn(crypto, "randomUUID").mockReturnValue(clientId);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

it.each([{ proposal: false }, { name: null }, { size: null }])(
  "renders nothing for an incomplete proposal: %j",
  (fields) => {
    const { container } = mount({ ...featureShape, ...fields });

    expect(container.childElementCount).toBe(0);
  },
);

it("confirms the proposed feature and waits for the command result", async () => {
  const { send, result } = mount();
  const section = screen.getByRole("region", { name: "Proposal" });
  expect(section.textContent).toContain("Proposed: a feature");
  expect(section.textContent).toContain("Name planner-search.");
  expect(section.classList.contains("font-sans")).toBe(true);
  expect(screen.getByText("planner-search").parentElement?.classList.contains("font-mono")).toBe(
    true,
  );

  fireEvent.click(screen.getByRole("button", { name: "Use this" }));

  expect(send).toHaveBeenCalledExactlyOnceWith({
    type: "chat",
    command: "confirm_kickoff",
    size: "feature",
    name: "planner-search",
    client_id: clientId,
  });
  for (const button of screen.getAllByRole("button")) {
    expect(button.getAttribute("aria-disabled")).toBe("true");
    fireEvent.click(button);
  }
  expect(send).toHaveBeenCalledOnce();
  await result();
  expect(screen.getByRole("button", { name: "Use this" }).hasAttribute("aria-disabled")).toBe(
    false,
  );
  expect(screen.queryByRole("status")).toBeNull();
  expect(screen.getByRole("region", { name: "Proposal" })).toBeTruthy();
});

it.each(["Planner-search", "plans"])("refuses the invalid rename %j", (name) => {
  const { send } = mount();
  fireEvent.click(screen.getByRole("button", { name: "Rename" }));
  const input = screen.getByRole("textbox", { name: "Name" }) as HTMLInputElement;
  expect(input.value).toBe("planner-search");
  expect(document.activeElement).toBe(input);

  fireEvent.change(input, { target: { value: name } });
  fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
  fireEvent.submit(input.closest("form")!);

  expect(screen.getByRole("alert").textContent).toBe(FEATURE_NAME_HINT);
  expect(input.getAttribute("aria-invalid")).toBe("true");
  expect(input.getAttribute("aria-describedby")).toBe(screen.getByRole("alert").id);
  expect(screen.getByRole("button", { name: "Confirm" }).getAttribute("aria-disabled")).toBe(
    "true",
  );
  expect(send).not.toHaveBeenCalled();
});

it("confirms a valid typed name after correcting an invalid one", async () => {
  const { send, result } = mount();
  fireEvent.click(screen.getByRole("button", { name: "Rename" }));
  const input = screen.getByRole("textbox", { name: "Name" });
  fireEvent.change(input, { target: { value: "plans" } });
  fireEvent.change(input, { target: { value: "search-v2" } });
  expect(screen.queryByRole("alert")).toBeNull();

  fireEvent.click(screen.getByRole("button", { name: "Confirm" }));

  expect(send).toHaveBeenCalledExactlyOnceWith({
    type: "chat",
    command: "confirm_kickoff",
    size: "feature",
    name: "search-v2",
    client_id: clientId,
  });
  expect((input as HTMLInputElement).disabled).toBe(true);
  expect(screen.getByRole("button", { name: "Cancel" }).getAttribute("aria-disabled")).toBe("true");
  await result();
  expect((input as HTMLInputElement).disabled).toBe(false);
});

it("cancels a rename without sending and restores the proposed name on reopening", () => {
  const { send } = mount();
  fireEvent.click(screen.getByRole("button", { name: "Rename" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Name" }), { target: { value: "other" } });

  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

  expect(screen.queryByRole("textbox")).toBeNull();
  expect(send).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Rename" }));
  expect((screen.getByRole("textbox", { name: "Name" }) as HTMLInputElement).value).toBe(
    "planner-search",
  );
});

it("offers the standalone plan label and confirms its size", async () => {
  const { send, result } = mount({ ...featureShape, size: "plan" });

  expect(screen.getByRole("region", { name: "Proposal" }).textContent).toContain(
    "Proposed: a standalone plan",
  );
  expect(screen.queryByRole("button", { name: "Change sections" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Make standalone" })).toBeNull();
  expect(screen.getByRole("button", { name: "Make a feature" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Use this" }));

  expect(send).toHaveBeenCalledExactlyOnceWith({
    type: "chat",
    command: "confirm_kickoff",
    size: "plan",
    name: "planner-search",
    client_id: clientId,
  });
  await result();
});

it.each([
  ["feature", "Change sections", "I want to change the proposed sections."],
  ["feature", "Make standalone", "Make this a standalone plan instead."],
  ["plan", "Make a feature", "Make this a feature instead."],
] as const)("sends the %s proposal's %s message", async (size, label, text) => {
  const { send, result } = mount({ ...featureShape, size });

  fireEvent.click(screen.getByRole("button", { name: label }));

  expect(send).toHaveBeenCalledExactlyOnceWith({
    type: "chat",
    command: "send",
    text,
    attachments: [],
    client_id: clientId,
  });
  expect(screen.getByRole("button", { name: label }).getAttribute("aria-disabled")).toBe("true");
  await result();
  expect(screen.getByRole("button", { name: label }).hasAttribute("aria-disabled")).toBe(false);
});

it("keeps actions blocked when the shape changes before the command result", async () => {
  const { store, send, result, rerender } = mount();
  fireEvent.click(screen.getByRole("button", { name: "Make standalone" }));

  rerender(
    <Provider store={store}>
      <Proposal shape={{ ...featureShape, size: "plan", name: "smaller-search" }} />
    </Provider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Use this" }));

  expect(screen.getByRole("button", { name: "Use this" }).getAttribute("aria-disabled")).toBe(
    "true",
  );
  expect(send).toHaveBeenCalledOnce();
  await result();
  expect(screen.getByRole("button", { name: "Use this" }).hasAttribute("aria-disabled")).toBe(
    false,
  );
});

it.each(["Use this", "Change sections"])(
  "shows a refusal from %s and allows retry",
  async (label) => {
    const { send, result } = mount();
    fireEvent.click(screen.getByRole("button", { name: label }));

    await result("<b>Name\u202e refused</b>");

    const status = screen.getByRole("status");
    expect(status.textContent).toBe("<b>Name⟨U+202E⟩ refused</b>");
    expect(status.firstElementChild?.getAttribute("style")).toContain("unicode-bidi: isolate");
    expect(status.querySelector("b")).toBeNull();
    expect(screen.getByRole("button", { name: label }).hasAttribute("aria-disabled")).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: label }));
    expect(send).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole("status")).toBeNull();
    await result();
  },
);

it("lists the proposed sections after the name, each through Untrusted", () => {
  mount({
    ...featureShape,
    sections: [
      { title: "Scope", state: "open" },
      { title: "<i>Order\u202e</i>", state: "open" },
    ],
  });

  const section = screen.getByRole("region", { name: "Proposal" });
  expect(section.textContent).toContain(
    "Name planner-search and 2 sections: Scope, <i>Order⟨U+202E⟩</i>.",
  );
  expect(screen.getByText("<i>Order⟨U+202E⟩</i>").style.unicodeBidi).toBe("isolate");
});

it("renders the model's proposed name through Untrusted", () => {
  mount({ ...featureShape, name: "<b>search\u202e</b>" });

  const name = screen.getByText("<b>search⟨U+202E⟩</b>");
  expect(name.style.unicodeBidi).toBe("isolate");
  expect(name.style.direction).toBe("ltr");
  expect(name.childElementCount).toBe(0);
});

it("shows the authoring holder's reason safely and sends nothing in read-only mode", () => {
  const store = proposalStore();
  store.set(authoringAtom, {
    holder: { public_id: "other", name: "Dana\u202e", away_since: null, frees_at: null },
  });
  const { send } = mount(featureShape, store);

  for (const button of screen.getAllByRole("button")) {
    expect(button.getAttribute("aria-disabled")).toBe("true");
    fireEvent.click(button);
  }

  const reason = screen.getByText("Dana⟨U+202E⟩ holds authoring");
  expect(reason.style.unicodeBidi).toBe("isolate");
  expect(screen.queryByRole("textbox")).toBeNull();
  expect(send).not.toHaveBeenCalled();
});

it("blocks confirmation if authoring is lost while renaming", () => {
  const { store, send } = mount();
  fireEvent.click(screen.getByRole("button", { name: "Rename" }));
  const input = screen.getByRole("textbox", { name: "Name" }) as HTMLInputElement;
  act(() => {
    store.set(authoringAtom, { holder: null });
    store.set(serverInfoAtom, serverInfoSchema.parse(hello.server));
  });

  fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
  fireEvent.submit(input.closest("form")!);

  expect(input.disabled).toBe(true);
  expect(screen.getByText("loom plan is in read-only mode: Claude is not logged in")).toBeTruthy();
  expect(send).not.toHaveBeenCalled();
});
