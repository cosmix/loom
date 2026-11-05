import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { SettingsDialog } from "@/plan/settings/settings-dialog";

const ENGLISH_PREVIEW =
  "The app stores no workflow state and works out the current step from the earliest gap in the doc/ tree.";
const SIZE_NAMES = ["Compact 17px", "Comfortable 19px", "Large 21px"];
const LINE_COUNTS = [5, 4, 3];

function stubNotifications(
  permission: NotificationPermission,
  result: NotificationPermission = permission,
) {
  const requestPermission = vi.fn(async () => result);
  class NotificationStub {
    static permission = permission;
    static requestPermission = requestPermission;
  }
  vi.stubGlobal("Notification", NotificationStub);
  vi.stubGlobal("isSecureContext", true);
  return requestPermission;
}

function renderSettings(store = createStore()) {
  return {
    store,
    ...render(
      <Provider store={store}>
        <SettingsDialog open onOpenChange={() => {}} />
      </Provider>,
    ),
  };
}

beforeEach(() => {
  localStorage.clear();
  stubNotifications("granted");
});

afterEach(() => {
  cleanup();
  localStorage.clear();
  vi.unstubAllGlobals();
});

/** The radios of `group`, checked to carry `names` as their accessible names in that order. */
function radiosNamed(group: HTMLElement, names: string[]): HTMLElement[] {
  const radios = within(group).getAllByRole("radio");
  expect(radios).toHaveLength(names.length);
  names.forEach((name, index) => {
    expect(within(group).getByRole("radio", { name }), name).toBe(radios[index]);
  });
  return radios;
}

it("each reading size option draws its own page picture", () => {
  renderSettings();
  const group = screen.getByRole("radiogroup", { name: "Reading size" });
  const radios = radiosNamed(group, SIZE_NAMES);
  radios.forEach((radio, index) => {
    const name = SIZE_NAMES[index]!;
    const label = radio.closest("label");
    expect(label, name).not.toBeNull();
    const pictures = label!.querySelectorAll("svg");
    expect(pictures, name).toHaveLength(1);
    const picture = pictures[0]!;
    expect(picture.getAttribute("aria-hidden"), name).toBe("true");
    expect(picture.getAttribute("width"), name).toBe("40");
    expect(picture.getAttribute("height"), name).toBe("32");
    expect(picture.querySelectorAll("[data-page]"), name).toHaveLength(1);
    const lines = picture.querySelectorAll("[data-line]");
    expect(lines, name).toHaveLength(LINE_COUNTS[index]!);
    if (index === 2) {
      const [first, second] = [lines[0]!, lines[1]!].map((line) =>
        Number(line.getAttribute("width")),
      );
      expect(first, "Large's first line is shorter than its second").toBeLessThan(second!);
    }
  });
});

it("the reading preview is a figure set above the face and size choices", () => {
  renderSettings();
  const figure = screen.getByRole("figure", { name: "Reading preview" });
  expect(figure.textContent).toContain("Preview");
  expect(figure.textContent).toContain(ENGLISH_PREVIEW);
  for (const name of ["Reading face", "Reading size"]) {
    const group = screen.getByRole("radiogroup", { name });
    const position = figure.compareDocumentPosition(group);
    expect(position & Node.DOCUMENT_POSITION_FOLLOWING, `${name} follows the preview`).not.toBe(0);
  }
  fireEvent.click(screen.getByRole("radio", { name: "Large 21px" }));
  const english = within(figure).getByText(ENGLISH_PREVIEW);
  expect(english.parentElement?.style.fontSize).toBe("21px");
});
