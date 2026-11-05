import { act, cleanup, render, screen } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { afterEach, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router";

import chatJson from "@/plan/api/fixtures/chat-snapshot.json";
import concept from "@/plan/api/fixtures/concept-summary.json";
import tree from "@/plan/api/fixtures/files-tree.json";
import { chatSnapshotSchema, type Attachment, type Item, type Turn } from "@/plan/api/schema-chat";
import { conceptPageSchema } from "@/plan/api/schema-concept";
import { fileTreeSchema } from "@/plan/api/schema-files";
import { addAttachmentAtom, type AttachmentDraft } from "@/plan/attachments";
import { conceptPageAtom } from "@/plan/concept/page-state";
import { treeAtom } from "@/plan/files/atoms";
import { serverInfoAtom } from "@/plan/state/atoms";
import { PendingChips, SentChips, chipLabel } from "./chips";
import { snapshotAtom } from "./model";

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
function setup() {
  const store = createStore();
  store.set(serverInfoAtom, {
    project: "p",
    project_key: `chips-marks-${++project}`,
    port: 1,
    remote: false,
    stopping: false,
    read_only: [],
    you: { public_id: "me", name: "Me" },
  });
  store.set(snapshotAtom, snapshot);
  return store;
}
function mount(store: Store, children = <PendingChips />, pathname = "/files") {
  return render(
    <Provider store={store}>
      <MemoryRouter initialEntries={[pathname]}>{children}</MemoryRouter>
    </Provider>,
  );
}
function add(store: Store, draft = source()) {
  act(() => store.set(addAttachmentAtom, draft));
}
function sent(store: Store, attachments: Attachment[], turn: Turn = baseTurn) {
  return mount(store, <SentChips item={{ ...baseUser, attachments }} turn={turn} />);
}
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
});

it("lock glyph for locked and knowledge docs", () => {
  const store = setup();
  store.set(treeAtom, {
    product: [],
    plans: [
      {
        feature: null,
        misplaced: false,
        path: "doc/plans/PLAN-foo.md",
        target: null,
        title: "Plan",
        file_name: "PLAN-foo.md",
        kind: "plan",
        state: "running",
        locked: true,
        gap: false,
        pressure_date: null,
        errors: 0,
        blurb: null,
        children: [],
      },
    ],
    knowledge: [],
    other: [],
    archive: [],
  });
  mount(store);
  add(store, source("doc/plans/PLAN-foo.md"));
  add(store, source("doc/loom/knowledge/topic.md"));
  add(store);
  const locks = screen.getAllByRole("img", { name: "Locked document" });
  expect(locks).toHaveLength(2);
  for (const lock of locks) {
    const glyph = lock.querySelector("svg.lucide-lock");
    expect(glyph).toBeTruthy();
    expect(glyph?.getAttribute("aria-hidden")).toBe("true");
  }
});

it.each([null, 3])("file chips show the kind's bare mark in the document's hue %s", (colour) => {
  const store = setup();
  store.set(
    treeAtom,
    fileTreeSchema.parse({
      ...tree,
      product: [
        {
          ...tree.product[0],
          children: [{ ...tree.product[0].children[0], kind: "section", path: concept.path }],
        },
      ],
    }),
  );
  const page = conceptPageSchema.parse({ ...concept, colour, key_chips: [] });
  if (colour !== null) store.set(conceptPageAtom(page.key), { page, previous: null, error: null });
  mount(store, <PendingChips />, "/f/demo/product/section_01");
  add(store, source(page.path!));
  const tray = screen.getByLabelText("Pending attachments");
  const mark = tray.querySelector(".lucide-book-open")?.parentElement;
  expect(mark).toBeTruthy();
  expect(mark?.getAttribute("aria-hidden")).toBe("true");
  // The hue colours the glyph itself; no tile stands behind it (DESIGN.md, one surface level).
  expect(mark?.className).toBe("chat-chip-glyph");
  expect(mark?.getAttribute("style")).toContain(`var(--doc-${colour ?? 2})`);
  expect(mark?.getAttribute("style")).not.toContain("background");
});

it("sent chips from model attachments", () => {
  const store = setup();
  sent(store, [{ ...baseAttachment, locked: "knowledge", changed_since_selected: true }], {
    ...baseTurn,
    open: true,
  });
  expect(screen.getByRole("button", { name: chipLabel(baseAttachment) })).toBeTruthy();
  expect(
    screen.getByRole("img", { name: "Locked document" }).querySelector("svg.lucide-lock"),
  ).toBeTruthy();
  expect(screen.getByText("△ changed")).toBeTruthy();
  expect(screen.queryByRole("button", { name: /remove/i })).toBeNull();
});
