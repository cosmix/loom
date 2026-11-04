import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import { panelLoadAtom, summaryReloadAtom, SummaryFeedHost } from "@/plan/summaries/state";
import { notificationsEnabledAtom } from "@/state/notifications";
import {
  autoRebuildAtom,
  READING_FACES,
  READING_SIZES,
  readingFaceAtom,
  readingSizeAtom,
  sourceMarksAtom,
} from "./preferences";
import { SettingsDialog } from "./settings-dialog";

const NOTIFICATION_LABEL = "Notify when a session needs me";
const ENGLISH_PREVIEW =
  "The app stores no workflow state and works out the current step from the earliest gap in the doc/ tree.";
const GREEK_PREVIEW = "Η εφαρμογή δεν αποθηκεύει κατάσταση ροής εργασίας.";

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

function SettingsLauncher() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        Open settings
      </button>
      <SettingsDialog open={open} onOpenChange={setOpen} />
    </>
  );
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

describe("planner settings sections", () => {
  it("renders the title, a hidden browser description, four headings and their hints", () => {
    renderSettings();

    const dialog = screen.getByRole("dialog", { name: "Settings" });
    expect(within(dialog).getByText("Saved in this browser").className).toContain("sr-only");
    for (const name of ["Theme", "Reading face", "Reading size", "Reading"]) {
      expect(within(dialog).getByRole("heading", { name })).toBeTruthy();
    }
    expect(within(dialog).queryByRole("heading", { name: "Reading toggles" })).toBeNull();
    expect(within(dialog).getByText("Remembered in this browser")).toBeTruthy();
    expect(
      within(dialog).getByText("Document text only; titles and controls stay Inter"),
    ).toBeTruthy();
    expect(screen.getByRole("radiogroup", { name: "Reading face" })).toBeTruthy();
    expect(screen.getByRole("radiogroup", { name: "Reading size" })).toBeTruthy();
  });

  it("uses all four planner swatches at 120 by 76, hidden from assistive technology", () => {
    renderSettings();

    const theme = screen.getByRole("group", { name: "theme" });
    const swatches = theme.querySelectorAll('svg[width="120"][height="76"][aria-hidden="true"]');
    expect(swatches).toHaveLength(4);
    for (const label of ["Ledger", "Aubergine", "Pacific", "Graphite"]) {
      const card = within(theme).getByRole("button", { name: `${label} theme` });
      expect(card.querySelector("svg")?.getAttribute("viewBox")).toBe("0 0 120 76");
      expect(card.querySelector('[data-region="spine"]')).toBeTruthy();
    }
  });

  it("shows each face specimen in its own family with its script coverage", () => {
    renderSettings();

    const group = screen.getByRole("radiogroup", { name: "Reading face" });
    expect(within(group).getAllByRole("radio")).toHaveLength(3);
    for (const face of READING_FACES) {
      const card = within(group).getByRole("radio", { name: face.label }).closest("label")!;
      const specimen = within(card).getByText("Aa").parentElement!;
      expect(specimen.textContent).toBe("AaΩω");
      expect(specimen.style.fontFamily).toBe(face.family);
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
    renderSettings(store);
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
      renderSettings(store);
      const radio = screen.getByRole("radio", { name: `${label} ${size}px` });

      fireEvent.click(radio);

      expect(store.get(readingSizeAtom)).toBe(size);
      expect(radio).toHaveProperty("checked", true);
      const group = screen.getByRole("radiogroup", { name: "Reading size" });
      expect(within(group).getAllByRole("radio", { checked: true })).toEqual([radio]);
    },
  );

  it("renders the size choices as a segmented control with the pressed segment checked", () => {
    renderSettings();

    const group = screen.getByRole("radiogroup", { name: "Reading size" });
    expect(group.className).toContain("ps-seg");
    const labels = within(group)
      .getAllByRole("radio")
      .map((radio) => radio.closest("label")!.textContent);
    expect(labels).toEqual(["Compact17px", "Comfortable19px", "Large21px"]);
    expect(within(group).getByRole("radio", { name: "Comfortable 19px" })).toHaveProperty(
      "checked",
      true,
    );
  });

  it("updates the bilingual preview from the selected face and size", () => {
    const store = createStore();
    store.set(readingFaceAtom, "source-serif");
    store.set(readingSizeAtom, 19);
    renderSettings(store);
    const preview = screen.getByText(ENGLISH_PREVIEW).parentElement!;

    expect(preview.style.fontFamily).toBe(READING_FACES[0]!.family);
    expect(preview.style.fontSize).toBe("19px");
    fireEvent.click(screen.getByRole("radio", { name: "Literata" }));
    expect(preview.style.fontFamily).toBe(READING_FACES[1]!.family);
    expect(preview.style.fontSize).toBe("19px");
    fireEvent.click(screen.getByRole("radio", { name: "Large 21px" }));
    expect(preview.style.fontFamily).toBe(READING_FACES[1]!.family);
    expect(preview.style.fontSize).toBe("21px");
    expect(preview.style.lineHeight).toBe("1.6");
    expect(screen.getByText(ENGLISH_PREVIEW).getAttribute("lang")).toBe("en");
    expect(screen.getByText(GREEK_PREVIEW).getAttribute("lang")).toBe("el");
  });

  it.each([
    ["Show source marks after sentences", sourceMarksAtom],
    ["Rebuild summaries automatically", autoRebuildAtom],
  ] as const)("toggles %s in the provided store", (label, preference) => {
    const store = createStore();
    store.set(preference, true);
    renderSettings(store);
    const toggle = screen.getByRole("switch", { name: label });

    fireEvent.click(toggle);

    expect(store.get(preference)).toBe(false);
    expect(toggle).toHaveProperty("checked", false);
    fireEvent.click(toggle);
    expect(store.get(preference)).toBe(true);
    expect(toggle).toHaveProperty("checked", true);
  });
});

describe("planner notification preference", () => {
  it("enables and disables the shared notification atom with permission already granted", () => {
    const requestPermission = stubNotifications("granted");
    const { store } = renderSettings();
    const toggle = screen.getByRole("switch", { name: NOTIFICATION_LABEL });

    fireEvent.click(toggle);

    expect(store.get(notificationsEnabledAtom)).toBe(true);
    expect(toggle).toHaveProperty("checked", true);
    expect(screen.getByRole("status").textContent).toBe("on");
    fireEvent.click(toggle);
    expect(store.get(notificationsEnabledAtom)).toBe(false);
    expect(toggle).toHaveProperty("checked", false);
    expect(requestPermission).not.toHaveBeenCalled();
  });

  it.each(["granted", "denied", "default"] as const)(
    "requests permission and handles a %s response",
    async (result) => {
      const requestPermission = stubNotifications("default", result);
      const { store } = renderSettings();
      const toggle = screen.getByRole("switch", { name: NOTIFICATION_LABEL });
      expect(screen.getByRole("status").textContent).toBe(
        "the browser will ask when you turn this on",
      );

      fireEvent.click(toggle);

      await waitFor(() => {
        expect(requestPermission).toHaveBeenCalledOnce();
        expect(store.get(notificationsEnabledAtom)).toBe(result === "granted");
        expect(toggle).toHaveProperty("checked", result === "granted");
        expect(toggle).toHaveProperty("disabled", result === "denied");
      });
    },
  );

  it.each([
    ["unsupported", "this browser has no notification support"],
    ["insecure", "notifications need localhost or HTTPS; this page is plain HTTP on a LAN address"],
    ["denied", "the browser has blocked notifications for this page"],
  ] as const)(
    "disables notifications when %s and clears a stale preference",
    async (support, message) => {
      if (support === "unsupported") Reflect.deleteProperty(window, "Notification");
      if (support === "insecure") vi.stubGlobal("isSecureContext", false);
      if (support === "denied") stubNotifications("denied");
      const store = createStore();
      store.set(notificationsEnabledAtom, true);
      renderSettings(store);
      const toggle = screen.getByRole("switch", { name: NOTIFICATION_LABEL });

      expect(toggle).toHaveProperty("disabled", true);
      expect(toggle).toHaveProperty("checked", false);
      expect(screen.getByRole("status").textContent).toBe(message);
      expect(toggle.getAttribute("aria-describedby")).toContain(screen.getByRole("status").id);
      await waitFor(() => expect(store.get(notificationsEnabledAtom)).toBe(false));
    },
  );
});

describe("planner settings focus", () => {
  it.each(["close button", "Escape"] as const)(
    "returns focus to the opener after %s",
    async (method) => {
      render(
        <Provider store={createStore()}>
          <SettingsLauncher />
        </Provider>,
      );
      expect(screen.queryByRole("dialog")).toBeNull();
      const opener = screen.getByRole("button", { name: "Open settings" });
      opener.focus();

      fireEvent.click(opener);

      const dialog = screen.getByRole("dialog", { name: "Settings" });
      expect(dialog.contains(document.activeElement)).toBe(true);
      if (method === "Escape") fireEvent.keyDown(dialog, { key: "Escape" });
      else fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));

      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
      await waitFor(() => expect(document.activeElement).toBe(opener));
    },
  );
});

it("Settings shows the summary totals", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(JSON.stringify(panelFixture), { status: 200 })),
  );
  const store = createStore();
  render(
    <Provider store={store}>
      <TooltipProvider>
        <SummaryFeedHost />
        <SettingsDialog open onOpenChange={() => {}} />
      </TooltipProvider>
    </Provider>,
  );
  const dialog = screen.getByRole("dialog", { name: "Settings" });
  expect(within(dialog).getByRole("heading", { name: "Summaries" })).toBeTruthy();

  expect(await within(dialog).findByRole("region", { name: "Token totals" })).toBeTruthy();
  expect(within(dialog).getByRole("img", { name: /^claude 5h window/ })).toBeTruthy();
});

it("Settings offers a retry when the summary feed failed", async () => {
  const reload = vi.fn();
  const store = createStore();
  store.set(panelLoadAtom, { status: "error", message: "HTTP 500" });
  store.set(summaryReloadAtom, { reload });
  render(
    <Provider store={store}>
      <SettingsDialog open onOpenChange={() => {}} />
    </Provider>,
  );

  const dialog = screen.getByRole("dialog", { name: "Settings" });
  fireEvent.click(within(dialog).getByRole("button", { name: "Retry" }));

  expect(reload).toHaveBeenCalledOnce();
});
