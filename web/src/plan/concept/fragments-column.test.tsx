import { fireEvent, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";

import type { AttachmentDraft } from "@/plan/attachments/types";

import { pageOf, S1, S2, sent, src } from "./reading-column-fixtures";
import {
  button,
  group,
  installColumnTestHooks,
  mount,
  mountProduct,
} from "./reading-column-harness";

installColumnTestHooks();

/** Three adjacent sentences opening to section_02's intro and detail, then a lone one. */
const sentences = () => [
  sent("a", "Alpha opens the intro.", [src(S2, "intro")]),
  sent("b", "Bravo opens both.", [src(S2, "intro"), src(S2, "detail")]),
  sent("c", "Charlie opens the detail.", [src(S2, "detail")]),
  sent("d", "Delta stands alone.", [src(S1, "intro")]),
];

it("adjacent sentences sharing a passage are one button, marked with their count", () => {
  const { column } = mountProduct(sentences());

  const fragment = button(column, /^Alpha opens the intro/);
  expect(fragment.textContent).toBe(
    "Alpha opens the intro. Bravo opens both. Charlie opens the detail.",
  );
  expect(fragment.closest(".concept-sentence-wrap")?.getAttribute("data-fragment")).toBe("3");
  expect(fragment.getAttribute("aria-expanded")).toBe("false");
  const single = button(column, /^Delta stands alone/);
  expect(single.closest(".concept-sentence-wrap")?.hasAttribute("data-fragment")).toBe(false);
  expect(column.querySelectorAll("[aria-expanded]")).toHaveLength(2);
});

it("each member keeps its own sentence id for selection", () => {
  const { column } = mountProduct(sentences());

  const owners = column.querySelectorAll(
    '.concept-sentence-wrap[data-fragment] [role="button"] [data-sentence-id]',
  );
  expect([...owners].map((owner) => owner.getAttribute("data-sentence-id"))).toEqual([
    "a",
    "b",
    "c",
  ]);
});

it("opening a fragment shows each of its passages once", async () => {
  const { column } = mountProduct(sentences());

  fireEvent.click(button(column, /^Alpha opens the intro/));

  const run = await group(column);
  const links = await within(run).findAllByRole("link", { name: /section_02\.md/ });
  expect(links.map((link) => link.textContent)).toEqual([
    expect.stringMatching(/Intro/),
    expect.stringMatching(/Detail/),
  ]);
  expect(button(column, /^Delta stands alone/).getAttribute("aria-expanded")).toBe("false");
});

it("the a key attaches one summary draft for every sentence of the fragment", () => {
  const onAttach = vi.fn<(draft: AttachmentDraft) => void>();
  const { column } = mount(
    "overview/demo/product",
    pageOf("overview/demo/product", sentences()),
    onAttach,
  );
  const fragment = button(column, /^Alpha opens the intro/);
  fragment.focus();

  fireEvent.keyDown(fragment, { key: "a" });

  expect(onAttach.mock.calls.map(([draft]) => draft.kind === "summary" && draft.sentence)).toEqual([
    "a",
    "b",
    "c",
  ]);
  expect(onAttach.mock.calls[1]![0]).toMatchObject({
    text: "Bravo opens both.",
    restsOn: [
      { path: S2, anchor: "intro" },
      { path: S2, anchor: "detail" },
    ],
  });
});
