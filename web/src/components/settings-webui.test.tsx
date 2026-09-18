import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { fakeClient, renderAt } from "@/test/settings-kit";

const originalMatchMedia = window.matchMedia;
const originalSecureContext = Object.getOwnPropertyDescriptor(window, "isSecureContext");

function stubNotifications(
  permission: NotificationPermission,
  requestResult: NotificationPermission = permission,
) {
  const requestPermission = vi.fn(async () => requestResult);

  class NotificationStub {
    static permission = permission;
    static requestPermission = requestPermission;
  }

  vi.stubGlobal("Notification", NotificationStub);
  Object.defineProperty(window, "isSecureContext", { configurable: true, value: true });
  return requestPermission;
}

function setNarrowBreakpoint() {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: (query: string) => ({
      matches: query === "(max-width: 699px)",
      media: query,
      onchange: null,
      addEventListener() {},
      removeEventListener() {},
      addListener() {},
      removeListener() {},
      dispatchEvent: () => false,
    }),
  });
}

afterEach(() => {
  cleanup();
  localStorage.clear();
  vi.unstubAllGlobals();
  Object.defineProperty(window, "matchMedia", { configurable: true, value: originalMatchMedia });
  if (originalSecureContext) {
    Object.defineProperty(window, "isSecureContext", originalSecureContext);
  } else {
    Reflect.deleteProperty(window, "isSecureContext");
  }
});

describe("dashboard settings", () => {
  it("renders the dashboard heading and four theme buttons", async () => {
    renderAt("?settings=1", fakeClient().client);

    expect(await screen.findByText("dashboard", { selector: ".eyebrow" })).toBeTruthy();
    expect(screen.getByRole("group", { name: "theme" }).querySelectorAll("button")).toHaveLength(4);
  });

  it("renders the desktop notifications switch", () => {
    renderAt("?settings=1", fakeClient().client);

    expect(screen.getByRole("switch", { name: "desktop notifications" })).toBeTruthy();
  });

  it.each([
    ["granted", "true"],
    ["denied", "false"],
  ] as const)("requests permission and leaves the switch %s", async (requestResult, checked) => {
    const requestPermission = stubNotifications("default", requestResult);
    renderAt("?settings=1", fakeClient().client);
    const toggle = await screen.findByRole("switch", { name: "desktop notifications" });

    fireEvent.click(toggle);

    await waitFor(() => expect(toggle.getAttribute("aria-checked")).toBe(checked));
    expect(requestPermission).toHaveBeenCalledOnce();
  });

  it("disables notifications on an insecure LAN origin", async () => {
    stubNotifications("default");
    Object.defineProperty(window, "isSecureContext", { configurable: true, value: false });
    renderAt("?settings=1", fakeClient().client);
    const toggle = (await screen.findByRole("switch", {
      name: "desktop notifications",
    })) as HTMLButtonElement;

    expect(toggle.disabled).toBe(true);
    expect(
      screen.getByText(
        "notifications need localhost or HTTPS; this page is plain HTTP on a LAN address",
      ),
    ).toBeTruthy();
  });

  it("keeps the dashboard section visible when configuration loading fails", async () => {
    renderAt("?settings=1", {
      load: () => Promise.reject(new Error("offline")),
      write: async () => ({ ok: false, status: null, message: "offline" }),
    });

    await screen.findByRole("alert");
    expect(screen.getByText("dashboard", { selector: ".eyebrow" })).toBeTruthy();
  });

  it("hides and shows the dashboard section while filtering", async () => {
    renderAt("?settings=1", fakeClient().client);
    await screen.findByRole("switch", { name: "check at user scope" });
    const filter = await screen.findByRole("searchbox", { name: "filter settings" });

    fireEvent.change(filter, { target: { value: "zzzzznotfound" } });

    expect(screen.queryByText("dashboard", { selector: ".eyebrow" })).toBeNull();
    expect(screen.getByText(/^nothing matches /)).toBeTruthy();

    fireEvent.change(filter, { target: { value: "theme" } });

    expect(screen.getByText("dashboard", { selector: ".eyebrow" })).toBeTruthy();
    expect(screen.queryByText(/^nothing matches /)).toBeNull();

    fireEvent.change(filter, { target: { value: "dark" } });

    expect(screen.getByText("dashboard", { selector: ".eyebrow" })).toBeTruthy();
  });

  it("reconciles a stale enabled preference when permission was denied", async () => {
    localStorage.setItem("loom:notifications", "true");
    stubNotifications("denied");
    renderAt("?settings=1", fakeClient().client);
    const toggle = (await screen.findByRole("switch", {
      name: "desktop notifications",
    })) as HTMLButtonElement;

    await waitFor(() => {
      expect(toggle.getAttribute("aria-checked")).toBe("false");
      expect(localStorage.getItem("loom:notifications")).toBe("false");
    });
    expect(toggle.disabled).toBe(true);
    expect(screen.getByText("the browser has blocked notifications for this page")).toBeTruthy();
  });

  it("turns an enabled granted notification preference off", async () => {
    localStorage.setItem("loom:notifications", "true");
    stubNotifications("granted");
    renderAt("?settings=1", fakeClient().client);
    const toggle = await screen.findByRole("switch", { name: "desktop notifications" });

    await waitFor(() => expect(toggle.getAttribute("aria-checked")).toBe("true"));
    fireEvent.click(toggle);

    expect(toggle.getAttribute("aria-checked")).toBe("false");
  });

  it.each(["wide", "narrow"] as const)(
    "filters dashboard and server sections at both breakpoints: %s",
    async (breakpoint) => {
      if (breakpoint === "narrow") setNarrowBreakpoint();
      renderAt("?settings=1", fakeClient().client);
      await screen.findByRole("switch", { name: "check at user scope" });
      const filter = await screen.findByRole("searchbox", { name: "filter settings" });

      if (breakpoint === "narrow") expect(document.querySelector(".settings-cards")).toBeTruthy();

      fireEvent.change(filter, { target: { value: "graphite" } });

      expect(screen.getByText("dashboard", { selector: ".eyebrow" })).toBeTruthy();
      expect(screen.queryByText(/^nothing matches /)).toBeNull();

      fireEvent.change(filter, { target: { value: "sonnet" } });

      expect(screen.queryByText("dashboard", { selector: ".eyebrow" })).toBeNull();
      expect(document.querySelectorAll(".settings-key-name").length).toBeGreaterThan(0);
      expect(screen.queryByText(/^nothing matches /)).toBeNull();

      fireEvent.change(filter, { target: { value: "zzzzznotfound" } });

      expect(screen.queryByText("dashboard", { selector: ".eyebrow" })).toBeNull();
      expect(screen.getAllByText(/^nothing matches /)).toHaveLength(1);
    },
  );
});
