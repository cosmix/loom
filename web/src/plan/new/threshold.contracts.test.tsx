import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { expect, it } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { checklistViewSchema, type ChecklistView } from "@/plan/api/schema";
import { harness, installStartChatLifecycle } from "@/plan/chat/start-chat-test-kit";
import { EmptyKickoff } from "@/plan/new/empty-kickoff";
import { projectAtom } from "@/plan/state/atoms";

installStartChatLifecycle();

const PICKER = "Which feature?";

function feature(patch: Record<string, unknown>): ChecklistView {
  return checklistViewSchema.parse({ ...checklistFixture.checklists[0]!, ...patch });
}

function plan(slug: string, state: string, path: string) {
  return { slug, path, title: `Plan ${slug}`, state, pressure_date: null, listed: true };
}

/** Every plan done, Plan follow-ups as the secondary, one open finding. */
function atlas(): ChecklistView {
  const done = (slug: string) => plan(slug, "done", `doc/plans/atlas/archive/DONE-PLAN-${slug}.md`);
  return feature({
    feature: "atlas",
    metaplans: [{ path: "doc/plans/atlas/index.md", title: "Atlas roadmap" }],
    plan_rows: [done("atlas-01"), done("atlas-02")],
    plans: { listed: 2, total: 2, written: 2, running: 0, done: 2 },
    untested: 0,
    current: { kind: "ready_all_done" },
    steps: checklistFixture.checklists[0]!.steps.map((step) => ({ ...step, mark: "done" })),
    primary: null,
    secondary: [
      { id: "plan-follow-ups", kind: "plan_follow_ups", label: "Plan follow-ups", target: null },
    ],
    items: [],
    candidates: [
      {
        id: "F-1",
        kind: "finding",
        stage: "Chat polish",
        stage_id: "chat-polish",
        text: "Settings statistics look bland",
        review: "doc/plans/REVIEW-PLAN-atlas-01.md",
      },
    ],
  });
}

function beacon(): ChecklistView {
  return feature({
    feature: "beacon",
    metaplans: [{ path: "doc/plans/beacon/index.md", title: "Beacon" }],
    plan_rows: [plan("beacon-01", "written", "doc/plans/beacon/PLAN-beacon-01.md")],
    current: { kind: "ready" },
    candidates: [],
  });
}

function comet(): ChecklistView {
  return feature({
    feature: "comet",
    metaplans: [],
    plan_rows: [],
    current: { kind: "step", number: 3 },
    candidates: [],
  });
}

function mount() {
  const h = harness();
  h.store.set(projectAtom, { checklists: [atlas(), beacon(), comet()], standalone: [], moves: [] });
  h.mount(
    <MemoryRouter>
      <EmptyKickoff />
    </MemoryRouter>,
  );
  return h;
}

function composer(): HTMLTextAreaElement {
  return screen.getByRole("textbox", { name: "What do you want to build?" }) as HTMLTextAreaElement;
}

function startButton(): HTMLElement {
  return screen.getByRole("button", { name: "Start the conversation" });
}

function startFrames(h: ReturnType<typeof harness>): unknown[] {
  return h.send.mock.calls
    .map(([frame]) => frame)
    .filter((frame) => (frame as { command?: unknown }).command === "start");
}

it("a follow-up starts only after a feature is picked", async () => {
  const h = mount();
  fireEvent.click(screen.getByRole("radio", { name: "Follow-up" }));

  const picker = await screen.findByRole("listbox", { name: PICKER });
  expect(within(picker).getByRole("option", { name: /atlas/ })).toBeTruthy();
  expect(within(picker).getByRole("option", { name: /beacon/ })).toBeTruthy();
  expect(within(picker).queryAllByRole("option", { name: /comet/ })).toEqual([]);

  fireEvent.change(composer(), { target: { value: "Tighten the settings dialog" } });
  expect(startButton().getAttribute("aria-disabled")).toBe("true");
  fireEvent.keyDown(composer(), { key: "Enter" });
  await act(async () => {});
  expect(startFrames(h)).toEqual([]);

  const open = screen.getByRole("listbox", { name: PICKER });
  fireEvent.click(within(open).getByRole("option", { name: /atlas/ }));
  expect(startButton().getAttribute("aria-disabled")).not.toBe("true");
  fireEvent.click(startButton());

  await waitFor(() =>
    expect(h.send).toHaveBeenCalledWith(
      expect.objectContaining({
        type: "chat",
        command: "start",
        offer: "plan-follow-ups",
        feature: "atlas",
        text: "Tighten the settings dialog",
      }),
    ),
  );
});

it("the feature picker works from the keyboard", async () => {
  mount();
  fireEvent.click(screen.getByRole("radio", { name: "Follow-up" }));

  const picker = await screen.findByRole("listbox", { name: PICKER });
  await waitFor(() => {
    const active = document.activeElement;
    expect(active === picker || (active !== null && picker.contains(active))).toBe(true);
  });

  fireEvent.keyDown(picker, { key: "ArrowDown" });
  fireEvent.keyDown(picker, { key: "Enter" });

  await waitFor(() => expect(screen.queryByRole("listbox", { name: PICKER })).toBeNull());
  expect(screen.getByRole("button", { name: "Change" })).toBeTruthy();
  expect(composer().placeholder.startsWith("What should change in beacon?")).toBe(true);
});
