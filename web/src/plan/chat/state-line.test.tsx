import { act, render, screen } from "@testing-library/react";
import { Provider } from "jotai/react";
import { expect, it, vi } from "vitest";

import helloFixture from "@/plan/api/fixtures/hello.json";
import { checklistViewSchema } from "@/plan/api/schema";
import type { Item, SessionView } from "@/plan/api/schema-chat";
import { registerOfferAction } from "@/plan/offers/actions";
import { checklistAtom, serverInfoAtom } from "@/plan/state/atoms";
import { FailureNotice } from "./failure-notice";
import { snapshotAtom } from "./model";
import {
  base,
  harness,
  permission,
  server,
  session,
  setupStateLineTests,
  since,
  stops,
  tool,
  turn,
} from "./state-line-kit";

setupStateLineTests();

const said: Item = {
  type: "user",
  id: "said",
  text: "Hello",
  attachments: [],
  delivery: "sent",
  origin: null,
  command: null,
};

it("every state's visible words", () => {
  const cases: Array<[SessionView["state"], string | null]> = [
    ["starting", "Starting"],
    ["idle", null],
    ["working", "Working"],
    ["permission", "⚠Needs your answer"],
    ["ending", "Wrapping up"],
    ["ended", "Ended"],
    ["failed", "Stopped"],
  ];
  for (const [state, words] of cases) {
    const { container, unmount } = harness(session({ state }), [turn([permission("Bash")])]).show();
    const row = container.querySelector(".chat-state-row");
    expect(row?.textContent ?? null, state).toBe(words);
    expect(screen.getAllByRole("status")[0].getAttribute("aria-live")).toBe("polite");
    unmount();
  }
});

it("an idle chat and no chat draw no visible element", () => {
  for (const current of [session({ state: "idle" }), null]) {
    const { container, unmount } = harness(current).show();
    expect(container.querySelector(".chat-state-row")).toBeNull();
    expect(container.textContent).not.toContain("Waiting for you");
    unmount();
  }
});

it("the line draws no roundel in any state", () => {
  for (const state of ["starting", "working", "ending", "idle", "permission"] as const) {
    const { container, unmount } = harness(session({ state })).show();
    expect(container.querySelector(".aurora-roundel"), state).toBeNull();
    unmount();
  }
});

it("a decision's warning glyph is hidden from assistive technology", () => {
  const { container } = harness(session({ state: "permission" })).show();
  const glyph = container.querySelector(".chat-state-glyph");
  expect(glyph?.textContent).toBe("⚠");
  expect(glyph?.getAttribute("aria-hidden")).toBe("true");
});

it("none draws no workflow offer; New chat is the panel head's", () => {
  const h = harness(null);
  const primary = {
    ...checklistViewSchema.parse(helloFixture.checklists[0]).primary!,
    label: "Write section 05",
  };
  const checklist = {
    feature: "demo",
    pitch: null,
    has_product: true,
    sections: [],
    plans: { listed: 0, total: 0, written: 0, running: 0, done: 0 },
    untested: 0,
    metaplans: [],
    plan_rows: [],
    current: { kind: "ready" as const },
    steps: [],
    primary,
    secondary: [],
    items: [],
  };
  stops.push(registerOfferAction(primary.kind, vi.fn()));
  h.store.set(checklistAtom, checklist);
  h.show();
  expect(screen.queryByRole("button", { name: "Write section 05" })).toBeNull();
  expect(screen.queryByText("Write section 05")).toBeNull();
  expect(screen.queryByRole("button", { name: "New chat" })).toBeNull();
});

it("two pending permissions are announced with both tools, once", () => {
  const h = harness(session({ state: "permission" }), [
    turn([permission("Edit")], { id: "newer", started_at: "2026-10-28T09:59:30Z" }),
    turn([permission("Bash")], { id: "older", started_at: since }),
  ]);
  const { container } = h.show();
  expect(container.textContent).toContain("Needs your answer");
  // ChatAnnouncer says it assertively, once; the polite region here would be a second announcement.
  expect(screen.getAllByRole("status")[0].textContent).toBe("");
  expect(screen.getByText("Needs your answer").closest("[aria-hidden='true']")).toBeNull();
});

it("the starting words are hidden from browse mode and the region names the session", () => {
  const { container } = harness(session({ state: "starting" })).show();
  expect(screen.getAllByRole("status")[0].textContent).toBe(
    `Starting session · ${base.name} · ${base.model}`,
  );
  expect(container.querySelector(".chat-state-row")?.getAttribute("aria-hidden")).toBe("true");
  expect(screen.queryByRole("button", { name: "Brief" })).toBeNull();
});

it("a failed spawn's notice", () => {
  const failed = session({ state: "failed", failure: { kind: "spawn", message: "tmux refused" } });
  const { container } = render(
    <Provider store={harness(failed).store}>
      <FailureNotice session={failed} />
    </Provider>,
  );
  expect(container.textContent).toContain("The session could not start");
  expect(screen.getByText("tmux refused")).toBeTruthy();
  expect(
    screen.getByText(
      "The documents keep what the session recorded as it went. A new chat starts fresh.",
    ),
  ).toBeTruthy();
  expect(container.querySelector('.chat-notice[data-tone="error"]')).toBeTruthy();
});

it("a died session's notice names its last activity", () => {
  const died = session({ state: "failed", failure: { kind: "died", message: "Process exited" } });
  const { container } = render(
    <Provider store={harness(died).store}>
      <FailureNotice session={died} />
    </Provider>,
  );
  expect(container.textContent).toContain("The session stopped unexpectedly");
  expect(container.textContent).toContain(`Last activity ${since} (41s ago)`);
});

it("a read-only server leaves the state line as it is: the banner carries the problems", () => {
  const h = harness(null);
  h.store.set(serverInfoAtom, {
    ...server,
    read_only: [
      { what: "tmux", message: "tmux missing", fix: "Install tmux" },
      { what: "claude_login", message: "Claude logged out", fix: "Log in to Claude" },
    ],
  });
  const { container } = h.show();
  for (const text of ["Read-only", "tmux missing", "Install tmux", "Claude logged out"]) {
    expect(container.textContent).not.toContain(text);
  }
  expect(container.querySelector(".chat-notice")).toBeNull();
  expect(container.querySelector(".chat-state-line")?.getAttribute("data-state")).toBe("none");
  expect(screen.queryByRole("button")).toBeNull();
});

it("announces state changes only, never elapsed time or tool summaries", () => {
  const h = harness(session({ state: "working" }), [turn([tool("First tool")])]);
  const { container } = h.show();
  const [status] = screen.getAllByRole("status");
  expect(status.textContent).toBe("Working");
  expect(container.textContent).not.toContain("First tool");
  act(() => vi.advanceTimersByTime(20_000));
  h.setTurns([turn([tool("Second tool")])]);
  expect(container.textContent).not.toContain("Second tool");
  expect(status.textContent).toBe("Working");
  h.setTurns([turn([said, tool("Second tool")])]);
  h.setSession(session({ state: "ending" }));
  expect(status.textContent).toBe(
    "Writing final notes into the documents · This can take up to 10 minutes.",
  );
  // An idle chat announces nothing; permission, ended and failed are ChatAnnouncer's.
  h.setSession(session({ state: "idle" }));
  expect(status.textContent).toBe("");
  h.setSession(session({ state: "permission" }));
  expect(status.textContent).toBe("");
  h.setSession(session({ state: "ended" }));
  expect(status.textContent).toBe("");
  h.setSession(session({ state: "failed", failure: { kind: "died", message: "gone" } }));
  expect(status.textContent).toBe("");
});

it("no chat announces itself in the polite region and draws no words", () => {
  const { container } = harness(null).show();
  expect(screen.getAllByRole("status")[0].textContent).toBe("No chat");
  expect(container.querySelector(".chat-state-row")).toBeNull();
});

const concluded = (fields: Partial<SessionView>) => ({
  session: null,
  turns: [],
  queue: [],
  previous: { session: session(fields), turns: [] },
});

it("a browser that connects after a chat failed shows Stopped, not No chat", () => {
  const h = harness(null);
  h.store.set(
    snapshotAtom,
    concluded({ state: "failed", failure: { kind: "died", message: "Process exited" } }),
  );
  const { container } = h.show();
  expect(container.textContent).toContain("Stopped");
  expect(container.textContent).not.toContain("No chat");
});

it("a browser that connects after a chat ended shows Ended, not No chat", () => {
  const h = harness(null);
  h.store.set(snapshotAtom, concluded({ state: "ended" }));
  const { container } = h.show();
  expect(container.textContent).not.toContain("No chat");
  // Exposed to browse mode and no status text of its own: ChatAnnouncer does the announcing.
  const line = screen.getByText("Ended");
  expect(line.closest("[aria-hidden='true']")).toBeNull();
});

it("a previous chat that was not concluded does not stand in for the current state", () => {
  const h = harness(null);
  h.store.set(snapshotAtom, concluded({ state: "idle" }));
  const { container } = h.show();
  expect(container.textContent).toContain("No chat");
});

it("session strings expose controls as visible text", () => {
  const h = harness(session({ state: "starting", name: "Plan‮name" }));
  const { container } = h.show();
  expect(container.textContent).toContain("Plan⟨U+202E⟩name");
  const failed = session({
    state: "failed",
    failure: { kind: "spawn", message: "Failed​here" },
  });
  const notice = render(
    <Provider store={h.store}>
      <FailureNotice session={failed} />
    </Provider>,
  );
  expect(notice.container.textContent).toContain("Failed⟨U+200B⟩here");
});

it("a chat the user never wrote to is announced as ending, with no closing write promised", () => {
  const h = harness(session({ state: "ending" }), [turn([tool("A tool")])]);
  h.show();
  const [status] = screen.getAllByRole("status");
  expect(status.textContent).toBe("Ending the chat");
});
