import { act, cleanup, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";

import helloFixture from "@/plan/api/fixtures/hello.json";
import { checklistViewSchema } from "@/plan/api/schema";
import type { SessionView } from "@/plan/api/schema-chat";
import { registerOfferAction } from "@/plan/offers/actions";
import { checklistAtom, serverInfoAtom } from "@/plan/state/atoms";
import { snapshotAtom } from "./model";
import {
  base,
  harness,
  now,
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

it("every state row", () => {
  const cases: Array<[SessionView["state"] | null, string]> = [
    [null, "No chat"],
    ["starting", "Starting session"],
    ["idle", "Waiting for you"],
    ["working", "Working · 41s"],
    ["permission", "Needs a decision: Bash"],
    ["ending", "Writing final notes into the documents"],
    ["ended", "Chat ended · 41s ago"],
    ["failed", "The session could not start"],
  ];
  for (const [state, words] of cases) {
    const h = harness(
      state ? session({ state, failure: { kind: "spawn", message: "spawn refused" } }) : null,
      [turn([permission("Bash")])],
    );
    const { container } = h.show();
    expect(container.textContent).toContain(words);
    expect(screen.getAllByRole("status")[0].getAttribute("aria-live")).toBe("polite");
    if (state === "starting") {
      expect(container.textContent).toContain(base.name);
      expect(container.textContent).toContain(base.model);
    }
    if (state === "ending") expect(container.textContent).toContain("up to 10 minutes");
    cleanup();
  }
});

it("none offers the checklist primary offer and New chat", () => {
  const h = harness(null);
  const primary = {
    ...checklistViewSchema.parse(helloFixture.checklists[0]).primary!,
    label: "Write section 05",
  };
  const checklist = {
    feature: "demo",
    pitch: null,
    title: "Demo",
    has_product: true,
    sections: [],
    plans: { listed: 0, total: 0, written: 0, running: 0, done: 0 },
    untested: 0,
    current: { kind: "ready" as const },
    steps: [],
    primary,
    secondary: [],
    items: [],
  };
  stops.push(registerOfferAction(primary.kind, vi.fn()));
  h.store.set(checklistAtom, checklist);
  h.show();
  expect(screen.getByRole("button", { name: "Write section 05" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "New chat" })).toBeTruthy();
});

it("working shows elapsed, newest tool, subagents", () => {
  const h = harness(
    session({ state: "working", subagents_running: 2, state_since: now.toISOString() }),
    [turn([tool("Old summary"), tool("Inspect doc/product.md")])],
  );
  const { container } = h.show();
  expect(container.textContent).toContain("Working · 41s · Inspect doc/product.md · 2 subagents");
  expect(container.textContent).not.toContain("Old summary");
  act(() => vi.advanceTimersByTime(1000));
  expect(container.textContent).toContain("Working · 42s");
});

it("working falls back to the open turn start", () => {
  const { container } = harness(
    session({ state: "working", working_since: null, state_since: now.toISOString() }),
  ).show();
  expect(container.textContent).toContain("Working · 41s");
});

it("starting shows the Brief link and no other state does", () => {
  const h = harness(session({ state: "starting" }));
  const { container } = h.show();
  expect(screen.getByRole("button", { name: "Brief" })).toBeTruthy();
  expect(container.textContent).toContain(`Starting session · ${base.name} · ${base.model}`);
  h.setSession(session({ state: "working" }));
  expect(screen.queryByRole("button", { name: "Brief" })).toBeNull();
});

it("two pending permissions name both tools and count 2", () => {
  const h = harness(session({ state: "permission" }), [
    turn([permission("Edit")], { id: "newer", started_at: "2026-10-28T09:59:30Z" }),
    turn([permission("Bash")], { id: "older", started_at: since }),
  ]);
  const { container } = h.show();
  expect(container.textContent).toContain("Needs 2 decisions: Bash, Edit");
  // ChatAnnouncer says it assertively, once; the polite region here would be a second announcement.
  expect(screen.getAllByRole("status")[0].textContent).toBe("");
  const words = screen.getByText("Needs 2 decisions: Bash, Edit");
  expect(words.closest("[aria-hidden='true']")).toBeNull();
});

it("failed spawn text", () => {
  const { container } = harness(
    session({ state: "failed", failure: { kind: "spawn", message: "tmux refused" } }),
  ).show();
  expect(container.textContent).toContain("The session could not start");
  expect(screen.getByText("tmux refused")).toBeTruthy();
  expect(
    screen.getByText(
      "The documents keep what the session recorded as it went. A new chat starts fresh.",
    ),
  ).toBeTruthy();
});

it("failed died text with last activity", () => {
  const { container } = harness(
    session({ state: "failed", failure: { kind: "died", message: "Process exited" } }),
  ).show();
  expect(container.textContent).toContain("The session stopped unexpectedly");
  expect(container.textContent).toContain(`Last activity ${since} (41s ago)`);
});

it("read-only mode hazard lists problems, no offers", () => {
  const h = harness(null);
  h.store.set(serverInfoAtom, {
    ...server,
    read_only: [
      { what: "tmux", message: "tmux missing", fix: "Install tmux" },
      { what: "claude_login", message: "Claude logged out", fix: "Log in to Claude" },
    ],
  });
  const { container } = h.show();
  for (const text of [
    "Read-only",
    "tmux missing",
    "Install tmux",
    "Claude logged out",
    "Log in to Claude",
  ]) {
    expect(container.textContent).toContain(text);
  }
  expect(container.querySelector(".hazard-warning")).toBeTruthy();
  expect(screen.queryByRole("button")).toBeNull();
});

it("announces state changes only, never elapsed time or tool summaries", () => {
  const h = harness(session({ state: "working" }), [turn([tool("First tool")])]);
  const { container } = h.show();
  const [status] = screen.getAllByRole("status");
  expect(status.textContent).toBe("Working");
  expect(container.textContent).toContain("Working · 41s · First tool");
  act(() => vi.advanceTimersByTime(20_000));
  h.setTurns([turn([tool("Second tool")])]);
  expect(container.textContent).toContain("Second tool");
  expect(status.textContent).toBe("Working");
  h.setSession(session({ state: "ending" }));
  expect(status.textContent).toBe(
    "Writing final notes into the documents · This can take up to 10 minutes.",
  );
  // Permission, ended and failed are ChatAnnouncer's, which stays mounted with the panel closed.
  h.setSession(session({ state: "permission" }));
  expect(status.textContent).toBe("");
  h.setSession(session({ state: "ended" }));
  act(() => vi.advanceTimersByTime(20_000));
  expect(status.textContent).toBe("");
  h.setSession(session({ state: "failed", failure: { kind: "died", message: "gone" } }));
  expect(status.textContent).toBe("");
});

it("no chat hides its duplicated words but keeps the buttons reachable", () => {
  const { container } = harness(null).show();
  const hidden = [...container.querySelectorAll("[aria-hidden='true']")].filter(
    (node) => node.textContent === "No chat",
  );
  expect(hidden).toHaveLength(1);
  expect(screen.getAllByRole("status")[0].textContent).toBe("No chat");
  expect(screen.getByRole("button", { name: "New chat" })).toBeTruthy();
});

const concluded = (fields: Partial<SessionView>) => ({
  session: null,
  turns: [],
  queue: [],
  previous: { session: session(fields), turns: [] },
});

it("a browser that connects after a chat failed shows the failure, not No chat", () => {
  const h = harness(null);
  h.store.set(
    snapshotAtom,
    concluded({ state: "failed", failure: { kind: "died", message: "Process exited" } }),
  );
  const { container } = h.show();
  expect(container.textContent).not.toContain("No chat");
  expect(container.textContent).toContain("The session stopped unexpectedly");
  expect(screen.getByText("Process exited")).toBeTruthy();
  expect(container.textContent).toContain(`Last activity ${since} (41s ago)`);
  expect(container.querySelector(".hazard-error")).toBeTruthy();
});

it("a browser that connects after a chat ended shows the Ended line, not No chat", () => {
  const h = harness(null);
  h.store.set(snapshotAtom, concluded({ state: "ended" }));
  const { container } = h.show();
  expect(container.textContent).not.toContain("No chat");
  // Exposed to browse mode and no status region of its own: ChatAnnouncer does the announcing.
  const line = screen.getByText("Chat ended · 41s ago");
  expect(line.closest("[aria-hidden='true'], [role='status']")).toBeNull();
});

it("a previous chat that was not concluded does not stand in for the current state", () => {
  const h = harness(null);
  h.store.set(snapshotAtom, concluded({ state: "idle" }));
  const { container } = h.show();
  expect(container.textContent).toContain("No chat");
});

it("hides only the duplicated state words, so the working detail stays reachable", () => {
  const h = harness(session({ state: "working", subagents_running: 1 }), [
    turn([tool("Inspect doc/product.md")]),
  ]);
  const { container } = h.show();
  const hidden = [...container.querySelectorAll("[aria-hidden='true']")].filter(
    (node) => node.textContent === "Working",
  );
  expect(hidden).toHaveLength(1);
  const exposed = screen.getByText(/41s · Inspect doc\/product\.md · 1 subagent/);
  expect(exposed.closest("[aria-hidden='true']")).toBeNull();
  h.setSession(session({ state: "idle" }));
  const idleHidden = [...container.querySelectorAll("[aria-hidden='true']")].map(
    (node) => node.textContent,
  );
  expect(idleHidden).toContain("Waiting for you");
});

it("session strings expose controls as visible text", () => {
  const h = harness(session({ state: "starting", name: "Plan\u202Ename" }));
  const { container } = h.show();
  expect(container.textContent).toContain("Plan⟨U+202E⟩name");
  h.setSession(
    session({ state: "failed", failure: { kind: "spawn", message: "Failed\u200Bhere" } }),
  );
  expect(container.textContent).toContain("Failed⟨U+200B⟩here");
});
