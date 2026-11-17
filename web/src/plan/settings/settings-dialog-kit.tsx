import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { afterEach, beforeEach, vi } from "vitest";

import { SettingsDialog } from "./settings-dialog";

export function stubNotifications(
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

export function renderSettings(store = createStore()) {
  return {
    store,
    ...render(
      <Provider store={store}>
        <SettingsDialog open onOpenChange={() => {}} />
      </Provider>,
    ),
  };
}

/** Chooses a rail tab, as a click on it does. */
export function openPane(name: string) {
  fireEvent.click(screen.getByRole("tab", { name }));
}

export function renderPane(name: string, store = createStore()) {
  const rendered = renderSettings(store);
  openPane(name);
  return rendered;
}

/** Clears storage and grants notifications before each test; restores globals after it. */
export function useSettingsTestLifecycle() {
  beforeEach(() => {
    localStorage.clear();
    stubNotifications("granted");
  });

  afterEach(() => {
    cleanup();
    localStorage.clear();
    vi.unstubAllGlobals();
  });
}
