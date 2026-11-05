import { fireEvent } from "@testing-library/react";
import { expect, it, vi } from "vitest";

import { ComposerArea } from "./composer";
import { box, installComposerLifecycle, mount, setup, ShortcutHost } from "./composer-harness";
import { DESKTOP, chatSheetOpenAtom } from "./sheet-state";

installComposerLifecycle();

function width(desktop: boolean) {
  vi.spyOn(window, "matchMedia").mockImplementation(
    (query) => ({ matches: query === DESKTOP ? desktop : false, media: query }) as MediaQueryList,
  );
}

function press() {
  const h = setup();
  mount(
    h.store,
    <>
      <ShortcutHost />
      <ComposerArea />
    </>,
  );
  fireEvent.keyDown(document.body, { key: "c" });
  return h.store;
}

it("c at desktop width focuses the composer and leaves the sheet closed", () => {
  width(true);
  const store = press();
  expect(document.activeElement).toBe(box());
  expect(store.get(chatSheetOpenAtom)).toBe(false);
});

it("c below 1280px opens the sheet", () => {
  width(false);
  const store = press();
  expect(store.get(chatSheetOpenAtom)).toBe(true);
});
