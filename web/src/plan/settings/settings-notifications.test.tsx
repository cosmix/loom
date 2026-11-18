import { fireEvent, screen, waitFor } from "@testing-library/react";
import { createStore } from "jotai";
import { describe, expect, it, vi } from "vitest";

import { notificationsEnabledAtom } from "@/state/notifications";
import { renderPane, stubNotifications, useSettingsTestLifecycle } from "./settings-dialog-kit";

const NOTIFICATION_LABEL = "Notify when a session needs me";

useSettingsTestLifecycle();

describe("planner notification preference", () => {
  it("enables and disables the shared notification atom with permission already granted", () => {
    const requestPermission = stubNotifications("granted");
    const { store } = renderPane("Notifications");
    const toggle = screen.getByRole("switch", { name: NOTIFICATION_LABEL });
    expect(screen.queryByRole("status")).toBeNull();

    fireEvent.click(toggle);

    expect(store.get(notificationsEnabledAtom)).toBe(true);
    expect(toggle).toHaveProperty("checked", true);
    // A switch that can do what it says prints no "on" or "off" line.
    expect(screen.queryByRole("status")).toBeNull();
    expect(screen.queryByText(/^(on|off)$/i)).toBeNull();
    fireEvent.click(toggle);
    expect(store.get(notificationsEnabledAtom)).toBe(false);
    expect(toggle).toHaveProperty("checked", false);
    expect(requestPermission).not.toHaveBeenCalled();
  });

  it.each(["granted", "denied", "default"] as const)(
    "requests permission and handles a %s response",
    async (result) => {
      const requestPermission = stubNotifications("default", result);
      const { store } = renderPane("Notifications");
      const toggle = screen.getByRole("switch", { name: NOTIFICATION_LABEL });
      expect(screen.getByRole("status").textContent).toBe(
        "The browser will ask when you turn this on",
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
    ["unsupported", "This browser has no notification support"],
    ["insecure", "Notifications need localhost or HTTPS; this page is plain HTTP on a LAN address"],
    ["denied", "The browser has blocked notifications for this page"],
  ] as const)(
    "disables notifications when %s and clears a stale preference",
    async (support, message) => {
      if (support === "unsupported") Reflect.deleteProperty(window, "Notification");
      if (support === "insecure") vi.stubGlobal("isSecureContext", false);
      if (support === "denied") stubNotifications("denied");
      const store = createStore();
      store.set(notificationsEnabledAtom, true);
      renderPane("Notifications", store);
      const toggle = screen.getByRole("switch", { name: NOTIFICATION_LABEL });

      expect(toggle).toHaveProperty("disabled", true);
      expect(toggle).toHaveProperty("checked", false);
      expect(screen.getByRole("status").textContent).toBe(message);
      expect(toggle.getAttribute("aria-describedby")).toContain(screen.getByRole("status").id);
      await waitFor(() => expect(store.get(notificationsEnabledAtom)).toBe(false));
    },
  );
});
