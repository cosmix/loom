import { cleanup, fireEvent, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

import { rememberPreview } from "@/plan/attachments/intake";
import { chipLabel, chipSource } from "./chips";
import { add, baseAttachment, baseTurn, mount, sent, setup, source } from "./chips-test-kit";
import { draftAtom } from "./drafts";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
});

it("labels per origin", () => {
  // A chip with no quote has nothing to quote: it says where the document is.
  const a = {
    ...baseAttachment,
    quote: "",
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

it("a selection reads as its quoted start, with the source as secondary text", () => {
  const selection = {
    ...baseAttachment,
    kind: "source" as const,
    path: "doc/product/section_04.md",
    anchor: "Heading",
    lines: [3, 9] as [number, number],
    stage: null,
    quote: `  The first   sentence\nof the selection runs on well past the forty-eight character cut`,
  };
  expect(chipLabel(selection)).toBe("“The first sentence of the selection runs on well…”");
  expect(chipSource(selection)).toBe("section_04.md › Heading · lines 3–9");
  const sentence = {
    ...selection,
    kind: "summary" as const,
    path: "doc/loom/summaries/demo/product/section_04.md",
  };
  expect(chipSource(sentence)).toBe("section_04 summary");
  expect(chipLabel({ ...selection, quote: "Short quote." })).toBe("“Short quote.”");
});

it("a whole document keeps its file name and has no secondary text", () => {
  const whole = {
    ...baseAttachment,
    kind: "source" as const,
    path: "doc/product/section_04.md",
    anchor: null,
    lines: null,
    stage: null,
  };
  expect(chipLabel(whole)).toBe("section_04.md");
  expect(chipSource(whole)).toBeNull();
});

it("a selection chip is one row: its quoted start, then its source, then the remove", () => {
  const store = setup();
  mount(store);
  add(store, source());
  const row = screen.getByLabelText("Pending attachments").querySelector(".chat-chip")!;
  const parts = [...row.children].map((child) => child.className);
  const open = parts.indexOf("chat-chip-open");
  const sourceAt = parts.indexOf("chat-chip-source");
  const remove = parts.indexOf("chat-chip-remove");
  expect(open).toBeGreaterThanOrEqual(0);
  expect(sourceAt).toBeGreaterThan(open);
  expect(remove).toBeGreaterThan(sourceAt);
  expect(remove).toBe(row.children.length - 1);
  expect(row.querySelector(".chat-chip-open")?.textContent).toBe("“The entire selected passage”");
  expect(row.querySelector(".chat-chip-source")?.textContent).toBe("section_02.md › Port");
});

it("a quoted label keeps its closing mark in its own element, outside the cut", () => {
  const store = setup();
  mount(store);
  const base = {
    kind: "source",
    path: "doc/product/section_02.md",
    anchor: "Port",
    lines: null,
    stageId: null,
    hash: "selected",
  } as const;
  add(store, {
    ...base,
    text: "A selection long enough that its chip has to cut it somewhere before the end",
  });
  add(store, { ...base, path: "doc/product/section_03.md", anchor: null, text: "" });
  const opens = [
    ...screen.getByLabelText("Pending attachments").querySelectorAll(".chat-chip-open"),
  ];
  expect(opens).toHaveLength(2);
  const [quoted, whole] = opens as [Element, Element];
  expect(quoted.getAttribute("data-quoted")).toBe("true");
  expect(quoted.querySelector(".chat-chip-quote")?.textContent).toBe(
    "“A selection long enough that its chip has to cut…",
  );
  expect(quoted.querySelector(".chat-chip-quote-mark")?.textContent).toBe("”");
  expect(quoted.textContent).toBe("“A selection long enough that its chip has to cut…”");
  // A whole document's label is no quote: one text run, no mark of its own.
  expect(whole.textContent).toBe("section_03.md");
  expect(whole.hasAttribute("data-quoted")).toBe(false);
  expect(whole.querySelector(".chat-chip-quote")).toBeNull();
});

it("a file chip reads its name and size, and removing it frees its thumbnail", () => {
  const store = setup();
  const revoke = vi.fn();
  vi.stubGlobal(
    "URL",
    Object.assign(URL, { createObjectURL: () => "blob:thumb", revokeObjectURL: revoke }),
  );
  rememberPreview("0123456789abcdef", new File(["x"], "sketch.png", { type: "image/png" }));
  mount(store);
  add(store, {
    kind: "file",
    id: "0123456789abcdef",
    name: "sketch.png",
    media_type: "image/png",
    bytes: 1234,
  });
  const tray = screen.getByLabelText("Pending attachments");
  expect(tray.textContent).toContain("sketch.png");
  expect(tray.textContent).toContain("1.2 KiB");
  expect(tray.querySelector("img")?.getAttribute("src")).toBe("blob:thumb");
  fireEvent.click(screen.getByRole("button", { name: "Remove sketch.png" }));
  expect(revoke).toHaveBeenCalledWith("blob:thumb");
  expect(store.get(draftAtom).attachments).toHaveLength(0);
  vi.unstubAllGlobals();
});

it.each([
  ["overview/demo/engineering", "Engineering overview"],
  ["plans/product", "Summary · product"],
  ["plans/engineering", "Summary · engineering"],
  ["demo/engineering/product", "Summary · product"],
  ["demo/engineering/engineering", "Summary · engineering"],
])("labels summary %s by its page kind", (id, label) => {
  expect(
    chipLabel({
      ...baseAttachment,
      quote: "",
      kind: "summary",
      path: `doc/loom/summaries/${id}.md`,
    }),
  ).toBe(label);
});

it("a sent file chip shows the name and size and fetches no section status", () => {
  const request = vi.spyOn(globalThis, "fetch");
  sent(
    setup(),
    [
      {
        ...baseAttachment,
        kind: "file",
        path: "",
        anchor: null,
        lines: null,
        quote: "",
        file: {
          id: "0123456789abcdef",
          name: "notes.pdf",
          media_type: "application/pdf",
          bytes: 2048,
        },
      },
    ],
    { ...baseTurn, open: true },
  );
  const tray = screen.getByLabelText("Sent attachments");
  expect(tray.textContent).toContain("notes.pdf");
  expect(tray.textContent).toContain("2.0 KiB");
  expect(screen.queryByRole("button", { name: /remove/i })).toBeNull();
  expect(request).not.toHaveBeenCalled();
});
