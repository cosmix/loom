import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { afterEach, expect, it, vi } from "vitest";

import checklist from "@/plan/api/fixtures/checklist.json";
import { projectChecklistViewSchema } from "@/plan/api/schema";
import { pendingAttachmentsAtom } from "@/plan/attachments";
import { attachmentsFromSelection } from "@/plan/concept/selection";
import { hashesAtom } from "@/plan/review/live";
import { projectAtom, serverInfoAtom } from "@/plan/state/atoms";
import { PendingChips } from "./chips";
import {
  add,
  baseAttachment,
  baseTurn,
  currentProjectKey,
  mount,
  sections,
  sent,
  setup,
  snapshot,
  source,
  summary,
} from "./chips-test-kit";
import { draftAtom } from "./drafts";
import { snapshotAtom } from "./model";
import { storageKey } from "./storage";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
});

it("hover full text, click opens, x removes", () => {
  const store = setup();
  mount(store);
  add(store);
  const chip = screen.getByRole("button", { name: "“The entire selected passage”" });
  fireEvent.mouseEnter(chip);
  expect(screen.getByRole("tooltip").textContent).toBe("The entire selected passage");
  fireEvent.mouseLeave(chip);
  expect(screen.queryByRole("tooltip")).toBeNull();
  fireEvent.focus(chip);
  expect(screen.getByRole("tooltip").textContent).toBe("The entire selected passage");
  fireEvent.click(chip);
  expect(screen.getByLabelText("Location").textContent).toBe("/files/product/section_02.md#Port");
  fireEvent.click(screen.getByRole("button", { name: "Remove “The entire selected passage”" }));
  expect(store.get(pendingAttachmentsAtom)).toEqual([]);
  expect(store.get(draftAtom).attachments).toEqual([]);
});

it.each(["pending", "sent"] as const)(
  "uses the reading face only for the %s attachment's quoted text",
  (origin) => {
    const store = setup();
    const quote = source().text;
    if (origin === "pending") {
      mount(store);
      add(store);
    } else {
      sent(store, [{ ...baseAttachment, quote }], { ...baseTurn, open: true });
    }
    const tray = screen.getByLabelText(
      origin === "pending" ? "Pending attachments" : "Sent attachments",
    );
    fireEvent.focus(within(tray).getAllByRole("button")[0]);
    const preview = screen.getByRole("tooltip");
    expect(preview.classList.contains("chat-passage")).toBe(true);
    expect(screen.getByText(quote).closest(".chat-passage")).toBe(preview);
    expect(preview.parentElement?.closest(".chat-passage")).toBeNull();
    expect(tray.closest(".chat-passage")).toBeNull();
    for (const button of within(tray).getAllByRole("button")) {
      expect(button.closest(".chat-passage")).toBeNull();
    }
  },
);

it("changed since selection", () => {
  const store = setup();
  mount(store);
  add(store);
  expect(screen.queryByText("△ changed")).toBeNull();
  act(() => store.set(hashesAtom, { head: null, docs: { "doc/product/section_02.md": "new" } }));
  expect(screen.getByText("△ changed")).toBeTruthy();
});

it.each([
  ["demo/product/section_01", "/f/demo/product/section_01#sentence-1"],
  ["plans/x", "/f/demo/engineering/x#sentence-1"],
  ["plans/y", "/plans/y#sentence-1"],
])("chips from file and concept modes accumulate and open %s", (id, href) => {
  const store = setup();
  store.set(
    projectAtom,
    projectChecklistViewSchema.parse({
      checklists: [
        {
          ...checklist.checklists[0],
          items: [
            { step: 4, text: "Plan x", path: "doc/plans/IN_PROGRESS-PLAN-x.md", offer: null },
          ],
        },
      ],
      standalone: [
        { slug: "y", path: "doc/plans/PLAN-y.md", state: "written", title: "Standalone" },
      ],
      moves: [],
    }),
  );
  mount(store);
  add(store);
  add(store, { ...summary(), summary: id });
  expect(screen.getByRole("button", { name: "“The entire selected passage”" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "“The summary passage”" }));
  expect(screen.getByLabelText("Location").textContent).toBe(href);
  expect(store.get(draftAtom).attachments).toHaveLength(2);
});

it("an invalid summary chip is plain text", () => {
  const store = setup();
  mount(store);
  add(store, { ...summary(), summary: "product/section_01" });
  expect(screen.getByText("“The summary passage”")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "“The summary passage”" })).toBeNull();
});

it("adding a chip sends nothing", () => {
  const fetch = vi.spyOn(globalThis, "fetch");
  const store = setup();
  mount(store);
  add(store);
  expect(fetch).not.toHaveBeenCalled();
  expect(store.get(snapshotAtom)).toEqual(snapshot);
});

it("spanning selection adds one chip per part", () => {
  const store = setup();
  const view = mount(
    store,
    <>
      <PendingChips />
      <div data-passage="doc/a.md#one">First part</div>
      <div data-passage="doc/b.md#two">Second part</div>
    </>,
  );
  const first = view.container.querySelector('[data-passage="doc/a.md#one"]')!;
  const second = view.container.querySelector('[data-passage="doc/b.md#two"]')!;
  const range = document.createRange();
  range.setStart(first.firstChild!, 0);
  range.setEnd(second.firstChild!, 11);
  const selection = window.getSelection()!;
  selection.removeAllRanges();
  selection.addRange(range);
  const drafts = attachmentsFromSelection(selection, () => undefined);
  drafts.forEach((draft) => add(store, draft));
  expect(
    store
      .get(pendingAttachmentsAtom)
      .map((draft) => (draft.kind === "file" ? draft.name : draft.text)),
  ).toEqual(["First part", "Second part"]);
  selection.removeAllRanges();
});

it("sent chip changed in this turn links to diff at heading", async () => {
  const request = sections([{ path: baseAttachment.path, anchors: [baseAttachment.anchor!] }]);
  sent(setup(), [baseAttachment]);
  const link = await screen.findByRole("link", { name: "△ changed in this turn" });
  const session = snapshot.session!.id;
  expect(link.getAttribute("href")).toBe(
    `/files/plans/reference-0.md?diff=turn&turn=${encodeURIComponent(`${session}/${baseTurn.id}`)}#overview`,
  );
  expect(String(request.mock.calls[0][0])).toContain(
    `/api/turn-sections?session=${session}&turn=${baseTurn.id}`,
  );
});

it("unchanged", async () => {
  sections([{ path: baseAttachment.path, anchors: ["another-heading"] }]);
  sent(setup(), [baseAttachment]);
  expect(await screen.findByText("unchanged")).toBeTruthy();
  expect(screen.queryByRole("link", { name: /changed in this turn/ })).toBeNull();
});

it("eleventh add is dropped with a note and two rows disclose the rest", () => {
  const store = setup();
  mount(store);
  for (let n = 1; n <= 11; n++) {
    add(store, { ...source(`doc/note-${n}.md`), text: `Selected text of note ${n}` });
  }
  const tray = within(screen.getByLabelText("Pending attachments"));
  expect(tray.getByRole("status").textContent).toBe("At most 10 attachments per message");
  expect(store.get(pendingAttachmentsAtom)).toHaveLength(10);
  expect(store.get(draftAtom).attachments).toHaveLength(10);
  expect(screen.queryByRole("button", { name: "“Selected text of note 11”" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "+7 more" }));
  expect(screen.getByRole("button", { name: "“Selected text of note 10”" })).toBeTruthy();
});

it("draft storage restores chips across stores and preserves text", async () => {
  const store = setup();
  store.set(draftAtom, { text: "Keep typing", attachments: [] });
  const view = mount(store);
  add(store);
  await waitFor(() =>
    expect(localStorage.getItem(storageKey(currentProjectKey(), "draft"))).toContain(
      "entire selected",
    ),
  );
  view.unmount();
  const restored = createStore();
  restored.set(serverInfoAtom, store.get(serverInfoAtom));
  mount(restored);
  expect(screen.getByRole("button", { name: "“The entire selected passage”" })).toBeTruthy();
  expect(restored.get(draftAtom).text).toBe("Keep typing");
});

it("clearing an accepted draft clears the pending selections", () => {
  const store = setup();
  mount(store);
  add(store);
  act(() => store.set(draftAtom, { text: "", attachments: [] }));
  expect(store.get(pendingAttachmentsAtom)).toEqual([]);
});

it("untrusted paths and preview text display format controls visibly", () => {
  const store = setup();
  mount(store);
  add(store, { ...source("doc/a\u202e.md"), text: "safe\u200btext" });
  const chip = screen.getByRole("button", { name: "“safe⟨U+200B⟩text”" });
  // The path is the secondary text now, and its control characters show as well.
  expect(screen.getByLabelText("Pending attachments").textContent).toContain("a⟨U+202E⟩.md › Port");
  fireEvent.focus(chip);
  expect(screen.getByRole("tooltip").textContent).toBe("safe⟨U+200B⟩text");
});

it("open turns do not fetch section status", () => {
  const request = vi.spyOn(globalThis, "fetch");
  sent(setup(), [baseAttachment], { ...baseTurn, open: true });
  expect(request).not.toHaveBeenCalled();
  expect(screen.queryByText("unchanged")).toBeNull();
});

it("previous session chips use the previous session even when turn ids repeat", async () => {
  const oldTurn = snapshot.previous!.turns[0];
  const request = sections([]);
  sent(setup(), [baseAttachment], oldTurn);
  await screen.findByText("unchanged");
  expect(String(request.mock.calls[0][0])).toContain(`session=${snapshot.previous!.session.id}`);
});

it("a failed section check is one status line in the server's words, with no box", async () => {
  vi.spyOn(globalThis, "fetch").mockResolvedValue(
    new Response(JSON.stringify({ error: "unavailable", message: "Sections unavailable" }), {
      status: 500,
    }),
  );
  sent(setup(), [baseAttachment]);
  const words = await screen.findByText("Sections unavailable");
  const line = words.closest(".chat-notice");
  expect(line?.getAttribute("role")).toBe("status");
  expect(line?.getAttribute("data-tone")).toBe("error");
});
