import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { afterEach, expect, it, vi } from "vitest";
import { MemoryRouter, useLocation } from "react-router";

import chatJson from "@/plan/api/fixtures/chat-snapshot.json";
import checklist from "@/plan/api/fixtures/checklist.json";
import { projectChecklistViewSchema } from "@/plan/api/schema";
import { chatSnapshotSchema, type Attachment, type Item, type Turn } from "@/plan/api/schema-chat";
import {
  addAttachmentAtom,
  pendingAttachmentsAtom,
  type AttachmentDraft,
} from "@/plan/attachments";
import { attachmentsFromSelection } from "@/plan/concept/selection";
import { hashesAtom } from "@/plan/review/live";
import { projectAtom, serverInfoAtom } from "@/plan/state/atoms";
import { PendingChips, SentChips, chipLabel } from "./chips";
import { draftAtom } from "./drafts";
import { snapshotAtom } from "./model";
import { storageKey } from "./storage";

type Store = ReturnType<typeof createStore>;
const snapshot = chatSnapshotSchema.parse(chatJson);
const baseTurn = snapshot.turns[0];
const baseUser = baseTurn.items.find(
  (item): item is Extract<Item, { type: "user" }> => item.type === "user",
)!;
const baseAttachment = baseUser.attachments[0];
let project = 0;

function source(path = "doc/product/section_02.md"): AttachmentDraft {
  return {
    kind: "source",
    path,
    anchor: "Port",
    text: "The entire selected passage",
    lines: null,
    stageId: null,
    hash: "selected",
  };
}
function summary(): Extract<AttachmentDraft, { kind: "summary" }> {
  return {
    kind: "summary",
    summary: "demo/product/section_01",
    sentence: "sentence-1",
    text: "The summary passage",
    restsOn: [],
    hash: null,
  };
}
function setup() {
  const store = createStore();
  store.set(serverInfoAtom, {
    project: "p",
    project_key: `chips-${++project}`,
    port: 1,
    remote: false,
    stopping: false,
    read_only: [],
    you: { public_id: "me", name: "Me" },
  });
  store.set(snapshotAtom, snapshot);
  return store;
}
function Location() {
  const location = useLocation();
  return (
    <output aria-label="Location">
      {location.pathname}
      {location.search}
      {location.hash}
    </output>
  );
}
function mount(store: Store, children = <PendingChips />, pathname = "/files") {
  return render(
    <Provider store={store}>
      <MemoryRouter initialEntries={[pathname]}>
        {children}
        <Location />
      </MemoryRouter>
    </Provider>,
  );
}
function add(store: Store, draft = source()) {
  act(() => store.set(addAttachmentAtom, draft));
}
function sent(store: Store, attachments: Attachment[], turn: Turn = baseTurn) {
  return mount(store, <SentChips item={{ ...baseUser, attachments }} turn={turn} />);
}
function sections(files: { path: string; anchors: string[] }[]) {
  return vi
    .spyOn(globalThis, "fetch")
    .mockResolvedValue(new Response(JSON.stringify({ turn: baseTurn.id, files })));
}
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
});

it("labels per origin", () => {
  const a = {
    ...baseAttachment,
    path: "doc/product/section_02.md",
    anchor: "Port",
    lines: null,
    stage: null,
  };
  expect(chipLabel(a)).toBe("section_02.md › Port");
  expect(chipLabel({ ...a, lines: [54, 60] })).toBe("section_02.md › Port · lines 54–60");
  expect(chipLabel({ ...a, path: "doc/plans/PLAN-foo.md", stage: "build" })).toBe(
    "PLAN-foo.md › stage build",
  );
  expect(chipLabel({ ...a, kind: "summary" })).toBe("Summary · section_02");
  expect(
    chipLabel({ ...a, kind: "summary", path: "doc/loom/summaries/overview/demo/product.md" }),
  ).toBe("Product overview");
});

it.each([
  ["overview/demo/engineering", "Engineering overview"],
  ["plans/product", "Summary · product"],
  ["plans/engineering", "Summary · engineering"],
  ["demo/engineering/product", "Summary · product"],
  ["demo/engineering/engineering", "Summary · engineering"],
])("labels summary %s by its page kind", (id, label) => {
  expect(
    chipLabel({ ...baseAttachment, kind: "summary", path: `doc/loom/summaries/${id}.md` }),
  ).toBe(label);
});

it("hover full text, click opens, x removes", () => {
  const store = setup();
  mount(store);
  add(store);
  const chip = screen.getByRole("button", { name: "section_02.md › Port" });
  fireEvent.mouseEnter(chip);
  expect(screen.getByRole("tooltip").textContent).toBe("The entire selected passage");
  fireEvent.mouseLeave(chip);
  expect(screen.queryByRole("tooltip")).toBeNull();
  fireEvent.focus(chip);
  expect(screen.getByRole("tooltip").textContent).toBe("The entire selected passage");
  fireEvent.click(chip);
  expect(screen.getByLabelText("Location").textContent).toBe("/files/product/section_02.md#Port");
  fireEvent.click(screen.getByRole("button", { name: "Remove section_02.md › Port" }));
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
  ["demo/product/section_01", "/f/demo/product/section_01#sentence-1", "section_01"],
  ["plans/x", "/f/demo/engineering/x#sentence-1", "x"],
  ["plans/y", "/plans/y#sentence-1", "y"],
])("chips from file and concept modes accumulate and open %s", (id, href, label) => {
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
  expect(screen.getByRole("button", { name: "section_02.md › Port" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: `Summary · ${label}` }));
  expect(screen.getByLabelText("Location").textContent).toBe(href);
  expect(store.get(draftAtom).attachments).toHaveLength(2);
});

it("an invalid summary chip is plain text", () => {
  const store = setup();
  mount(store);
  add(store, { ...summary(), summary: "product/section_01" });
  expect(screen.getByText("Summary · section_01")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Summary · section_01" })).toBeNull();
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
  expect(store.get(pendingAttachmentsAtom).map((draft) => draft.text)).toEqual([
    "First part",
    "Second part",
  ]);
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
  for (let n = 1; n <= 11; n++) add(store, source(`doc/note-${n}.md`));
  const tray = within(screen.getByLabelText("Pending attachments"));
  expect(tray.getByRole("status").textContent).toBe("At most 10 attachments per message");
  expect(store.get(pendingAttachmentsAtom)).toHaveLength(10);
  expect(store.get(draftAtom).attachments).toHaveLength(10);
  expect(screen.queryByRole("button", { name: "note-11.md › Port" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "+7 more" }));
  expect(screen.getByRole("button", { name: "note-10.md › Port" })).toBeTruthy();
});

it("draft storage restores chips across stores and preserves text", async () => {
  const store = setup();
  store.set(draftAtom, { text: "Keep typing", attachments: [] });
  const view = mount(store);
  add(store);
  await waitFor(() =>
    expect(localStorage.getItem(storageKey(`chips-${project}`, "draft"))).toContain(
      "entire selected",
    ),
  );
  view.unmount();
  const restored = createStore();
  restored.set(serverInfoAtom, store.get(serverInfoAtom));
  mount(restored);
  expect(screen.getByRole("button", { name: "section_02.md › Port" })).toBeTruthy();
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
  const chip = screen.getByRole("button", { name: "a⟨U+202E⟩.md › Port" });
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
