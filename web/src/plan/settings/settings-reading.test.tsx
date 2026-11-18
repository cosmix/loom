import { fireEvent, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { describe, expect, it } from "vitest";

import {
  autoRebuildAtom,
  READING_FACES,
  READING_SIZES,
  readingFaceAtom,
  readingSizeAtom,
  sourceMarksAtom,
} from "./preferences";
import { renderPane, useSettingsTestLifecycle } from "./settings-dialog-kit";

const ENGLISH_PREVIEW =
  "The app stores no workflow state and works out the current step from the earliest gap in the doc/ tree.";
const GREEK_PREVIEW = "Η εφαρμογή δεν αποθηκεύει κατάσταση ροής εργασίας.";

// Compare the CSSOM serialization, which may omit optional quotes around family names.
function cssFontFamily(family: string): string {
  const { style } = document.createElement("span");
  style.fontFamily = family;
  return style.fontFamily;
}

useSettingsTestLifecycle();

describe("planner reading settings", () => {
  it("shows each face specimen in its own family with its script coverage", () => {
    renderPane("Reading");

    const group = screen.getByRole("radiogroup", { name: "Reading face" });
    expect(within(group).getAllByRole("radio")).toHaveLength(3);
    for (const face of READING_FACES) {
      const card = within(group).getByRole("radio", { name: face.label }).closest("label")!;
      const specimen = within(card).getByText("Aa").parentElement!;
      expect(specimen.textContent).toBe("AaΩω");
      expect(specimen.style.fontFamily).toBe(cssFontFamily(face.family));
      expect(within(card).getByText(face.label)).toBeTruthy();
      expect(within(card).getByText(face.scripts)).toBeTruthy();
    }
    const flags = screen.getAllByText("Default");
    expect(flags).toHaveLength(1);
    const defaultCard = screen.getByRole("radio", { name: "Source Serif 4" }).closest("label")!;
    expect(defaultCard.contains(flags[0]!)).toBe(true);
  });

  it.each(READING_FACES)("selects $label and writes the reading face atom", (face) => {
    const store = createStore();
    store.set(readingFaceAtom, face.id === "piazzolla" ? "source-serif" : "piazzolla");
    renderPane("Reading", store);
    const radio = screen.getByRole("radio", { name: face.label });

    fireEvent.click(radio);

    expect(store.get(readingFaceAtom)).toBe(face.id);
    expect(radio).toHaveProperty("checked", true);
    const group = screen.getByRole("radiogroup", { name: "Reading face" });
    expect(within(group).getAllByRole("radio", { checked: true })).toEqual([radio]);
  });

  it.each(READING_SIZES)(
    "selects $label $size px and writes the reading size atom",
    ({ size, label }) => {
      const store = createStore();
      store.set(readingSizeAtom, size === 19 ? 17 : 19);
      renderPane("Reading", store);
      const radio = screen.getByRole("radio", { name: `${label} ${size}px` });

      fireEvent.click(radio);

      expect(store.get(readingSizeAtom)).toBe(size);
      expect(radio).toHaveProperty("checked", true);
      const group = screen.getByRole("radiogroup", { name: "Reading size" });
      expect(within(group).getAllByRole("radio", { checked: true })).toEqual([radio]);
    },
  );

  it("renders the size choices as a row of tiles, each a picture over its word and px value", () => {
    renderPane("Reading");

    const group = screen.getByRole("radiogroup", { name: "Reading size" });
    expect(group.className).toContain("ps-seg");
    const tiles = within(group)
      .getAllByRole("radio")
      .map((radio) => radio.closest("label")!);
    expect(tiles.map((tile) => tile.className)).toEqual(Array(3).fill("ps-seg-item"));
    // The picture is an SVG without text, so the tile's text is its word and px value only.
    expect(tiles.map((tile) => tile.textContent)).toEqual([
      "Compact17px",
      "Comfortable19px",
      "Large21px",
    ]);
    expect(tiles.map((tile) => tile.querySelectorAll("svg [data-line]").length)).toEqual([5, 4, 3]);
    expect(within(group).getByRole("radio", { name: "Comfortable 19px" })).toHaveProperty(
      "checked",
      true,
    );
  });

  it("updates the bilingual preview from the selected face and size", () => {
    const store = createStore();
    store.set(readingFaceAtom, "source-serif");
    store.set(readingSizeAtom, 19);
    renderPane("Reading", store);
    const preview = screen.getByText(ENGLISH_PREVIEW).parentElement!;

    expect(preview.style.fontFamily).toBe(cssFontFamily(READING_FACES[0]!.family));
    expect(preview.style.fontSize).toBe("19px");
    fireEvent.click(screen.getByRole("radio", { name: "Literata" }));
    expect(preview.style.fontFamily).toBe(cssFontFamily(READING_FACES[1]!.family));
    expect(preview.style.fontSize).toBe("19px");
    fireEvent.click(screen.getByRole("radio", { name: "Large 21px" }));
    expect(preview.style.fontFamily).toBe(cssFontFamily(READING_FACES[1]!.family));
    expect(preview.style.fontSize).toBe("21px");
    expect(preview.style.lineHeight).toBe("1.6");
    expect(screen.getByText(ENGLISH_PREVIEW).getAttribute("lang")).toBe("en");
    expect(screen.getByText(GREEK_PREVIEW).getAttribute("lang")).toBe("el");
  });

  it("frames the preview as a specimen with a hidden eyebrow, above the face and size choices", () => {
    renderPane("Reading");

    const figure = screen.getByRole("figure", { name: "Reading preview" });
    expect(figure.className).toBe("ps-preview");
    const eyebrow = figure.querySelector("figcaption")!;
    expect(eyebrow.textContent).toBe("Preview");
    expect(eyebrow.getAttribute("aria-hidden")).toBe("true");
    const panel = screen.getByRole("tabpanel", { name: "Reading" });
    expect(panel.contains(figure)).toBe(true);
    const face = screen.getByRole("radiogroup", { name: "Reading face" });
    expect(figure.compareDocumentPosition(face) & Node.DOCUMENT_POSITION_FOLLOWING).not.toBe(0);
  });

  it.each([
    ["Reading", "Show source marks after sentences", sourceMarksAtom],
    ["Summaries", "Rebuild summaries automatically", autoRebuildAtom],
  ] as const)("on %s, toggles %s in the provided store", (pane, label, preference) => {
    const store = createStore();
    store.set(preference, true);
    renderPane(pane, store);
    const toggle = screen.getByRole("switch", { name: label });

    fireEvent.click(toggle);

    expect(store.get(preference)).toBe(false);
    expect(toggle).toHaveProperty("checked", false);
    fireEvent.click(toggle);
    expect(store.get(preference)).toBe(true);
    expect(toggle).toHaveProperty("checked", true);
  });
});
