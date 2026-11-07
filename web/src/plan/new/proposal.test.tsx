import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import hello from "@/plan/api/fixtures/hello.json";
import type { KickoffShape } from "@/plan/api/schema-kickoff";
import type { connectPlanSocket } from "@/plan/api/socket";
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

it("states the proposed feature and offers only Change sections", () => {
  mount();

  const section = screen.getByRole("region", { name: "Proposal" });
  expect(section.textContent).toContain("Proposed: a feature");
  expect(section.textContent).toContain("Name planner-search.");
  expect(section.classList.contains("font-sans")).toBe(true);
  expect(screen.getByText("planner-search").parentElement?.classList.contains("font-mono")).toBe(
    true,
  );
  expect(screen.getAllByRole("button").map((button) => button.textContent)).toEqual([
    "Change sections",
  ]);
});

it("states the proposed standalone plan with no control at all", () => {
  mount({ ...featureShape, size: "plan" });

  expect(screen.getByRole("region", { name: "Proposal" }).textContent).toContain(
    "Proposed: a standalone plan",
  );
  expect(screen.queryAllByRole("button")).toEqual([]);
});

it.each(["Use this", "Rename", "Make standalone", "Make a feature"])(
  "draws no %s control for either size",
  (label) => {
    for (const size of ["feature", "plan"] as const) {
      const { unmount } = mount({ ...featureShape, size });
      expect(screen.queryByRole("button", { name: label })).toBeNull();
      unmount();
    }
  },
);

it("sends the Change sections message once and waits for the result", async () => {
  const { send, result } = mount();
  const button = () => screen.getByRole("button", { name: "Change sections" });

  fireEvent.click(button());
  fireEvent.click(button());

  expect(send).toHaveBeenCalledExactlyOnceWith({
    type: "chat",
    command: "send",
    text: "I want to change the proposed sections.",
    attachments: [],
    client_id: clientId,
  });
  expect(button().getAttribute("aria-disabled")).toBe("true");
  await result();
  expect(button().hasAttribute("aria-disabled")).toBe(false);
});

it("shows a refusal from Change sections and allows retry", async () => {
  const { send, result } = mount();
  fireEvent.click(screen.getByRole("button", { name: "Change sections" }));

  await result("<b>Name\u202e refused</b>");

  const status = screen.getByRole("status");
  expect(status.textContent).toBe("<b>Name⟨U+202E⟩ refused</b>");
  expect(status.firstElementChild?.getAttribute("style")).toContain("unicode-bidi: isolate");
  expect(status.querySelector("b")).toBeNull();
  const button = screen.getByRole("button", { name: "Change sections" });
  expect(button.hasAttribute("aria-disabled")).toBe(false);
  fireEvent.click(button);
  expect(send).toHaveBeenCalledTimes(2);
  expect(screen.queryByRole("status")).toBeNull();
  await result();
});

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
  expect(send).not.toHaveBeenCalled();
});
