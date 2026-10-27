import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import shareFixture from "@/plan/api/fixtures/share.json";
import { Share } from "@/plan/header/share";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  Reflect.deleteProperty(navigator, "clipboard");
});

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function stubShare(body: unknown, status = 200) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => jsonResponse(body, status)),
  );
}

/** `undefined` is what a non-secure context (a plain-HTTP remote bind) exposes. */
function stubClipboard(writeText: ((text: string) => Promise<void>) | undefined) {
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: writeText ? { writeText } : undefined,
  });
}

async function openShare() {
  stubShare(shareFixture);
  render(<Share />);
  fireEvent.click(screen.getByRole("button", { name: /^share/i }));
  await screen.findByText(shareFixture.url);
}

describe("Share", () => {
  it("shows the tunnel line on a loopback bind", async () => {
    stubShare(shareFixture);
    render(<Share />);
    fireEvent.click(screen.getByRole("button", { name: /^share/i }));
    expect(await screen.findByText(shareFixture.url)).toBeTruthy();
    expect(screen.getByRole("dialog", { name: "Share link" })).toBeTruthy();
    expect(screen.getByText(/From another machine, tunnel first:/)).toBeTruthy();
    expect(screen.getByText(shareFixture.tunnel)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Copy tunnel command" })).toBeTruthy();
    expect(screen.queryByText(/plain HTTP/)).toBeNull();
  });

  it("shows the plain-HTTP warning on a remote bind", async () => {
    stubShare({ url: "http://10.0.0.5:7272/", bind: "remote", tunnel: null });
    render(<Share />);
    fireEvent.click(screen.getByRole("button", { name: /^share/i }));
    expect(await screen.findByText("http://10.0.0.5:7272/")).toBeTruthy();
    expect(screen.getByText(/plain HTTP/)).toBeTruthy();
    expect(screen.queryByText(/tunnel first/)).toBeNull();
  });

  it("reports a failed fetch", async () => {
    stubShare({ error: "unauthenticated" }, 401);
    render(<Share />);
    fireEvent.click(screen.getByRole("button", { name: /^share/i }));
    expect((await screen.findByRole("alert")).textContent).toContain("Could not load the link");
  });

  it("announces the loading line as a status", () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() => new Promise<Response>(() => {})),
    );
    render(<Share />);
    fireEvent.click(screen.getByRole("button", { name: /^share/i }));
    expect(screen.getByRole("status").textContent).toBe("Loading link");
  });

  it("shows loading again on reopen instead of the previous tokenized link", async () => {
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockImplementationOnce(async () => jsonResponse(shareFixture))
      .mockImplementation(() => new Promise<Response>(() => {}));
    vi.stubGlobal("fetch", fetchMock);
    render(<Share />);
    const trigger = screen.getByRole("button", { name: /^share/i });
    fireEvent.click(trigger);
    expect(await screen.findByText(shareFixture.url)).toBeTruthy();
    fireEvent.click(trigger);
    await waitFor(() => expect(screen.queryByText(shareFixture.url)).toBeNull());
    fireEvent.click(trigger);
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
    expect(screen.getByText("Loading link")).toBeTruthy();
    expect(screen.queryByText(shareFixture.url)).toBeNull();
  });

  it("announces a successful copy and names the button after its visible text", async () => {
    const writeText = vi.fn<(text: string) => Promise<void>>(async () => {});
    stubClipboard(writeText);
    await openShare();
    fireEvent.click(screen.getByRole("button", { name: "Copy link" }));
    const status = await screen.findByText("Copied the link to the clipboard.");
    expect(status.getAttribute("role")).toBe("status");
    expect(writeText).toHaveBeenCalledWith(shareFixture.url);
    expect(screen.getByRole("button", { name: "Copied" }).textContent).toBe("Copied");
  });

  it("says the copy failed and selects the link when there is no clipboard", async () => {
    stubClipboard(undefined);
    await openShare();
    fireEvent.click(screen.getByRole("button", { name: "Copy link" }));
    const status = await screen.findByText("Copy failed. Select the link and copy it.");
    expect(status.getAttribute("role")).toBe("status");
    expect(window.getSelection()?.toString()).toBe(shareFixture.url);
    expect(screen.getByRole("button", { name: "Copy link" }).textContent).toBe("Copy");
  });
});
