import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

import { Share } from "@/plan/header/share";

const LINK = "http://127.0.0.1:7272/?token=t";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("the share popover shows the link and no tunnel", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(
      async () =>
        new Response(JSON.stringify({ url: LINK, bind: "loopback" }), {
          headers: { "content-type": "application/json" },
        }),
    ),
  );
  render(<Share />);
  fireEvent.click(screen.getByRole("button", { name: /^share/i }));

  expect(await screen.findByRole("button", { name: "Copy link" })).toBeTruthy();
  expect(screen.getByText(LINK)).toBeTruthy();
  expect(screen.queryByText(/tunnel/i)).toBeNull();
  expect(screen.queryByRole("button", { name: /tunnel/i })).toBeNull();
});
