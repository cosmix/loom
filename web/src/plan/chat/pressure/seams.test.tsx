import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { Provider } from "jotai";
import type { ReactNode } from "react";
import { MemoryRouter } from "react-router";
import { beforeEach, expect, it, vi } from "vitest";

import runFixture from "@/plan/api/fixtures/pressure-run.json";
import type { EndedChat, Item, SessionView } from "@/plan/api/schema-chat";
import { pressureRunSchema, type PressureRunView } from "@/plan/api/schema-pressure";
import { ComposerArea } from "../composer";
import { composerBlock } from "../composer-gate";
import { initial, installComposerLifecycle, setup } from "../composer-harness";
import { snapshotAtom } from "../model";
import { ResendControl } from "../resend-control";
import { RunHeader } from "../run-header";
import { SessionDivider } from "../session-divider";
import { pressureRunAtom } from "./use-pressure";

const codexReason = "Codex is reviewing; send when the next Claude step starts";
const pressureSession: SessionView = {
  ...initial.session!,
  id: "address-session",
  kind: "pressure",
  state: "working",
};
const previous: EndedChat = {
  session: { ...pressureSession, id: "pressure-session", state: "ended" },
  turns: [],
};

function addressRun(): PressureRunView {
  const run = pressureRunSchema.parse(runFixture);
  return {
    ...run,
    step: "claude_address",
    steps: [
      ...run.steps,
      {
        ...run.steps[0],
        kind: "claude_address",
        session_id: pressureSession.id,
        model: "claude-opus",
        state: "running",
        ended_ms: null,
      },
    ],
  };
}

function mount(children: ReactNode, store = setup("working").store) {
  return render(children, {
    wrapper: ({ children }) => (
      <Provider store={store}>
        <MemoryRouter>{children}</MemoryRouter>
      </Provider>
    ),
  });
}

installComposerLifecycle();
beforeEach(() => {
  vi.spyOn(Date, "now").mockReturnValue(4000);
});

it("RunHeader switches between the normal and pressure headers with unconditional hooks", () => {
  const { store } = setup("working");
  store.set(pressureRunAtom, pressureRunSchema.parse(runFixture));
  const view = mount(<RunHeader session={initial.session!} />, store);
  expect(screen.getByText("authoring").textContent).toContain("Me authoring");
  expect(screen.queryByText("Pressure test")).toBeNull();

  view.rerender(<RunHeader session={pressureSession} />);

  expect(screen.getByText("Pressure test")).toBeTruthy();
  expect(screen.getByText("round 2 of 2")).toBeTruthy();
  expect(
    within(document.querySelector<HTMLElement>(".chat-run-header")!).getByText("PLAN-example.md"),
  ).toBeTruthy();
  expect(screen.queryByText("authoring")).toBeNull();

  view.rerender(<RunHeader session={initial.session!} />);
  expect(screen.getByText("authoring").textContent).toContain("Me authoring");
});

it("SessionDivider names the next pressure step and model", () => {
  const { store } = setup();
  store.set(pressureRunAtom, addressRun());

  mount(<SessionDivider previous={previous} next={pressureSession} />, store);

  expect(screen.getByText("/address · claude-opus")).toBeTruthy();
  expect(screen.queryByText(/^Chat ended/)).toBeNull();
});

it.each([
  { name: "unmapped next session", next: { ...pressureSession, id: "unmapped" }, run: true },
  { name: "no next session", next: null, run: true },
  { name: "normal next session", next: { ...pressureSession, kind: "authoring" }, run: true },
  { name: "no pressure run", next: pressureSession, run: false },
])("SessionDivider keeps the old text with $name", ({ next, run }) => {
  const { store } = setup();
  store.set(pressureRunAtom, run ? addressRun() : null);

  mount(<SessionDivider previous={previous} next={next} />, store);

  expect(screen.getByText(/^Chat ended at \d{2}:\d{2}$/)).toBeTruthy();
});

it("SessionDivider keeps the old text when the previous session is normal", () => {
  const { store } = setup();
  store.set(pressureRunAtom, addressRun());
  const normal = { ...previous, session: { ...previous.session, kind: "authoring" } };

  mount(<SessionDivider previous={normal} next={pressureSession} />, store);

  expect(screen.getByText(/^Chat ended at \d{2}:\d{2}$/)).toBeTruthy();
});

it("SessionDivider keeps the undated fallback when no step maps", () => {
  const undated = { ...previous, session: { ...previous.session, state_since: "unknown" } };

  mount(<SessionDivider previous={undated} next={null} />);

  expect(screen.getByText("Chat ended")).toBeTruthy();
});

it.each([
  { codexAlone: true, expected: codexReason },
  { codexAlone: false, expected: null },
  { codexAlone: undefined, expected: null },
])("composerBlock uses the Codex reason with codexAlone=$codexAlone", (options) => {
  expect(
    composerBlock(pressureSession, {
      stopping: false,
      readOnly: false,
      codexAlone: options.codexAlone,
    }),
  ).toBe(options.expected);
});

it.each([
  { state: "working" as const, stopping: true, expected: "loom plan is stopping" },
  { state: "ending" as const, stopping: false, expected: "the chat is closing" },
])("composerBlock keeps $expected ahead of the Codex reason", ({ state, stopping, expected }) => {
  expect(
    composerBlock({ ...pressureSession, state }, { stopping, readOnly: false, codexAlone: true }),
  ).toBe(expected);
});

it("composerBlock keeps read-only blocking when Codex is not alone", () => {
  expect(composerBlock(pressureSession, { stopping: false, readOnly: true })).toBe("Read-only");
});

it.each(["pressure", "authoring"])(
  "composer Stop visibility follows the %s session kind",
  (kind) => {
    const { store } = setup("working");
    store.set(snapshotAtom, (snapshot) => ({
      ...snapshot,
      session: { ...pressureSession, kind },
    }));

    mount(<ComposerArea />, store);

    expect(screen.queryByRole("button", { name: "Stop" }) !== null).toBe(kind === "authoring");
  },
);

it("composer Send is blocked during Codex alone and enabled for the next Claude step", () => {
  const { store } = setup("working");
  store.set(snapshotAtom, (snapshot) => ({ ...snapshot, session: pressureSession }));
  store.set(pressureRunAtom, pressureRunSchema.parse(runFixture));
  mount(<ComposerArea />, store);
  fireEvent.change(screen.getByRole("textbox", { name: "Message" }), {
    target: { value: "Continue" },
  });
  const send = screen.getByRole("button", { name: "Send", description: codexReason });
  expect(send.getAttribute("aria-disabled")).toBe("true");

  act(() => store.set(pressureRunAtom, addressRun()));

  expect(screen.getByRole("button", { name: "Send" }).hasAttribute("aria-disabled")).toBe(false);
  expect(screen.queryByText(codexReason)).toBeNull();
});

it("Resend is hidden during Codex alone and returns for the next Claude step", () => {
  const { store } = setup("working");
  const item: Extract<Item, { type: "user" }> = {
    type: "user",
    id: "lost",
    text: "Continue",
    attachments: [],
    delivery: "undelivered",
    origin: null,
  };
  const turn = { ...initial.turns[0], items: [item] };
  store.set(snapshotAtom, (snapshot) => ({ ...snapshot, session: pressureSession, turns: [turn] }));
  store.set(pressureRunAtom, pressureRunSchema.parse(runFixture));
  mount(<ResendControl item={item} turn={turn} />, store);
  expect(screen.queryByRole("button", { name: "Resend" })).toBeNull();

  act(() => store.set(pressureRunAtom, addressRun()));

  expect(screen.getByRole("button", { name: "Resend" })).toBeTruthy();
});
