import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { expect, it, vi } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { checklistViewSchema, type ChecklistView } from "@/plan/api/schema";
import { harness, installStartChatLifecycle } from "@/plan/chat/start-chat-test-kit";
import type { StarterDocument } from "@/plan/new/document-starter";
import { EmptyKickoff } from "@/plan/new/empty-kickoff";
import { projectAtom } from "@/plan/state/atoms";

const { notes } = vi.hoisted(() => ({
  notes: {
    label: "My notes",
    attachment: {
      kind: "source",
      path: "kickoff/notes.md",
      anchor: "",
      lines: null,
      stage: null,
      summary_sentence: null,
      rests_on: [],
      quote: "A place to collect project ideas.",
      selected_hash: null,
    },
  } satisfies StarterDocument,
}));

vi.mock("./document-starter", () => ({
  DocumentStarter: ({
    onPick,
    children,
  }: {
    onPick: (document: StarterDocument) => void;
    children: ReactNode;
  }) => (
    <button type="button" onClick={() => onPick(notes)}>
      {children}
    </button>
  ),
}));

installStartChatLifecycle();

const PICKER = "Which feature?";
const SEED =
  "Settings statistics look bland (open finding from REVIEW-PLAN-atlas-01.md, stage chat-polish)";

function feature(patch: Record<string, unknown>): ChecklistView {
  return checklistViewSchema.parse({ ...checklistFixture.checklists[0]!, ...patch });
}

function plan(slug: string, state: string, path: string) {
  return { slug, path, title: `Plan ${slug}`, state, pressure_date: null, listed: true };
}

/** Built, with one open finding. */
function atlas(): ChecklistView {
  const done = (slug: string) => plan(slug, "done", `doc/plans/atlas/archive/DONE-PLAN-${slug}.md`);
  return feature({
    feature: "atlas",
    metaplans: [{ path: "doc/plans/atlas/index.md", title: "Atlas roadmap" }],
    plan_rows: [done("atlas-01"), done("atlas-02")],
    plans: { listed: 2, total: 2, written: 2, running: 0, done: 2 },
    current: { kind: "ready_all_done" },
    primary: null,
    secondary: [],
    items: [],
    built: {
      reviews: [{ slug: "atlas-01", path: "doc/plans/REVIEW-PLAN-atlas-01.md" }],
      spec: [],
      candidates: [
        {
          kind: "finding",
          stage: "Chat polish",
          stage_id: "chat-polish",
          text: "Settings statistics look bland",
          review: "doc/plans/REVIEW-PLAN-atlas-01.md",
        },
      ],
    },
  });
}

/** Has a metaplan, is not built, has no findings. */
function beacon(): ChecklistView {
  return feature({
    feature: "beacon",
    metaplans: [{ path: "doc/plans/beacon/index.md", title: "Beacon" }],
    plan_rows: [plan("beacon-01", "written", "doc/plans/beacon/PLAN-beacon-01.md")],
    current: { kind: "ready" },
    built: { reviews: [], spec: [], candidates: [] },
  });
}

function mount(props: { followUp?: string | null; onFollowUp?: (feature: string) => void } = {}) {
  const h = harness();
  h.store.set(projectAtom, { checklists: [atlas(), beacon()], standalone: [], moves: [] });
  h.mount(<EmptyKickoff {...props} />);
  return h;
}

const radio = (name: string) => screen.getByRole("radio", { name }) as HTMLInputElement;
const composer = () =>
  screen.getByRole("textbox", { name: "What do you want to build?" }) as HTMLTextAreaElement;
const startButton = () => screen.getByRole("button", { name: "Start the conversation" });
const picker = () => screen.getByRole("listbox", { name: PICKER });
const pickAtlas = () => fireEvent.click(within(picker()).getByRole("option", { name: /atlas/ }));

it("the follow-up starter chooses Follow-up, keeps the words and opens the picker", () => {
  mount();
  fireEvent.change(composer(), { target: { value: "Keep me" } });

  fireEvent.click(
    screen.getByRole("button", { name: "New plans for a feature that is already built" }),
  );

  expect(radio("Follow-up").checked).toBe(true);
  expect(composer().value).toBe("Keep me");
  expect(picker()).toBeTruthy();
  expect(screen.queryByText("Or begin with one of these")).toBeNull();
});

it("a follow-up without a feature blocks Start with Pick a feature first", () => {
  const h = mount();
  fireEvent.click(radio("Follow-up"));
  fireEvent.keyDown(picker(), { key: "Escape" });

  expect(screen.queryByRole("listbox", { name: PICKER })).toBeNull();
  expect(document.activeElement).toBe(radio("Follow-up"));
  fireEvent.change(composer(), { target: { value: "Something" } });
  expect(startButton().getAttribute("aria-disabled")).toBe("true");
  expect(screen.getByText("Pick a feature first")).toBeTruthy();
  fireEvent.click(startButton());
  expect(h.send).not.toHaveBeenCalled();
});

it("a picked feature's candidate pill appends its seed on its own line and removes it again", () => {
  mount();
  fireEvent.click(radio("Follow-up"));
  pickAtlas();

  expect(screen.getByRole("button", { name: "Change" })).toBeTruthy();
  expect(composer().placeholder).toBe(
    "What should change in atlas? Pick a finding below, or say it in your own words.",
  );
  expect(document.querySelector(".kickoff-starters__label")?.textContent).toBe(
    "Open findings from atlas's reviews. Each one you pick goes into the field.",
  );
  const pill = screen.getByRole("button", { name: "Settings statistics look bland" });
  expect(pill.getAttribute("aria-pressed")).toBe("false");

  fireEvent.change(composer(), { target: { value: "First line" } });
  fireEvent.click(pill);
  expect(composer().value).toBe(`First line\n${SEED}`);
  expect(pill.getAttribute("aria-pressed")).toBe("true");

  fireEvent.click(pill);
  expect(composer().value).toBe("First line");
  expect(pill.getAttribute("aria-pressed")).toBe("false");
});

it("a picked feature with no findings says so and asks plainly", () => {
  mount();
  fireEvent.click(radio("Follow-up"));
  fireEvent.click(within(picker()).getByRole("option", { name: /beacon/ }));

  expect(composer().placeholder).toBe("What should change in beacon?");
  expect(document.querySelector(".kickoff-starters__none")?.textContent).toBe(
    "No open findings in beacon's run reviews.",
  );
  expect(screen.queryByRole("button", { name: /Settings statistics/ })).toBeNull();
});

it("Change reopens the picker and a press outside closes it with the feature kept", () => {
  mount();
  fireEvent.click(radio("Follow-up"));
  pickAtlas();
  expect(screen.queryByRole("listbox", { name: PICKER })).toBeNull();

  fireEvent.click(screen.getByRole("button", { name: "Change" }));
  expect(picker()).toBeTruthy();
  fireEvent.pointerDown(composer());

  expect(screen.queryByRole("listbox", { name: PICKER })).toBeNull();
  expect(screen.getByRole("button", { name: "Change" })).toBeTruthy();
  expect(startButton().getAttribute("aria-disabled")).toBe("true");
  expect(screen.getByText("Describe what you want to build or attach a document")).toBeTruthy();
});

it("a dismissed picker is reopened by Pick a feature, which gives way to Change once picked", () => {
  mount();
  expect(screen.queryByRole("button", { name: "Pick a feature" })).toBeNull();
  fireEvent.click(radio("Follow-up"));
  fireEvent.keyDown(picker(), { key: "Escape" });
  expect(screen.queryByRole("listbox", { name: PICKER })).toBeNull();

  fireEvent.click(screen.getByRole("button", { name: "Pick a feature" }));
  expect(picker()).toBeTruthy();
  pickAtlas();

  expect(screen.queryByRole("button", { name: "Pick a feature" })).toBeNull();
  expect(screen.getByRole("button", { name: "Change" })).toBeTruthy();
});

it("followUp preselects a listed feature and opens with it picked", () => {
  mount({ followUp: "atlas" });
  expect(radio("Follow-up").checked).toBe(true);
  expect(screen.getByRole("button", { name: "Change" })).toBeTruthy();
  expect(screen.queryByRole("listbox", { name: PICKER })).toBeNull();
  expect(composer().placeholder.startsWith("What should change in atlas?")).toBe(true);
});

it("ignores a followUp feature that has no metaplan", () => {
  mount({ followUp: "comet" });
  expect(radio("Feature").checked).toBe(true);
  expect(screen.queryByRole("button", { name: "Change" })).toBeNull();
});

it("starts Plan follow-ups for the picked feature and tells the route once it has started", async () => {
  const onFollowUp = vi.fn();
  const h = mount({ onFollowUp });
  fireEvent.click(radio("Follow-up"));
  pickAtlas();
  fireEvent.click(screen.getByRole("button", { name: "Settings statistics look bland" }));
  fireEvent.click(startButton());

  expect(h.send).toHaveBeenCalledWith(
    expect.objectContaining({
      command: "start",
      offer: "plan-follow-ups",
      feature: "atlas",
      text: SEED,
    }),
  );
  expect(onFollowUp).not.toHaveBeenCalled();
  h.result();
  await act(async () => {});
  await waitFor(() => expect(onFollowUp).toHaveBeenCalledExactlyOnceWith("atlas"));
});

it("switching back to Feature brings the starters back and keeps the field's words", () => {
  mount();
  fireEvent.click(radio("Follow-up"));
  pickAtlas();
  fireEvent.change(composer(), { target: { value: "Still here" } });

  fireEvent.click(radio("Feature"));

  expect(screen.getByText("Or begin with one of these")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Change" })).toBeNull();
  expect(composer().value).toBe("Still here");
  expect(composer().placeholder).toBe("Say what's on your mind…");
});

it("an attached document reads as the chat composer's row: glyph, name, file name, remove", () => {
  mount();
  fireEvent.click(screen.getByRole("button", { name: "Attach a document" }));
  const row = screen.getByRole("listitem");
  expect(row.classList.contains("chat-chip")).toBe(true);
  const parts = [...row.children].map((child) => child.className);
  expect(parts[0]).toBe("chat-chip-glyph");
  expect(parts.indexOf("chat-chip-label")).toBe(1);
  expect(parts.indexOf("chat-chip-source")).toBe(2);
  expect(parts[parts.length - 1]).toBe("chat-chip-remove");
  expect(row.querySelector(".chat-chip-source")?.textContent).toBe("notes.md");
  const remove = within(row).getByRole("button", { name: "Remove" });
  expect(remove.getAttribute("aria-describedby")).toBe(row.querySelector(".chat-chip-label")?.id);
});
