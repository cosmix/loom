import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import shareFixture from "@/plan/api/fixtures/share.json";
import { Share } from "@/plan/header/share";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function stubShare(body: unknown, status = 200) {
  vi.stubGlobal(
    "fetch",
    vi.fn(
      async () =>
        new Response(JSON.stringify(body), {
          status,
          headers: { "content-type": "application/json" },
        }),
    ),
  );
}

describe("Share", () => {
  it("shows the tunnel line on a loopback bind", async () => {
    stubShare(shareFixture);
    render(<Share />);
    fireEvent.click(screen.getByRole("button", { name: /^share/i }));
    expect(await screen.findByText(shareFixture.url)).toBeTruthy();
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
});
