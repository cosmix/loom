import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { BusyText } from "./busy-text";

afterEach(cleanup);

describe("BusyText", () => {
  it("leads the words with the text-sized arrow spinner, never the roundel", () => {
    const { container } = render(<BusyText>Sending…</BusyText>);

    const root = container.firstElementChild!;
    expect(root.classList.contains("plan-busy-text")).toBe(true);
    const lead = root.firstElementChild!;
    expect(lead.classList.contains("plan-arrow-spinner")).toBe(true);
    expect(lead.getAttribute("aria-hidden")).toBe("true");
    expect(lead.getAttribute("data-busy")).toBe("true");
    expect(lead.getAttribute("style")).toContain("width: 1em");
    expect(container.querySelector(".aurora-roundel")).toBeNull();
    expect(root.lastChild?.textContent).toBe("Sending…");
    expect(screen.getByText("Sending…")).toBe(root);
  });

  it("announces nothing of its own: the caller's words and role are the message", () => {
    render(<BusyText>Uploading…</BusyText>);

    expect(screen.queryByRole("status")).toBeNull();
    expect(document.body.textContent).toBe("Uploading…");
  });

  it("forwards a class name to its root", () => {
    const { container } = render(<BusyText className="chat-wait">Waiting</BusyText>);

    expect(container.firstElementChild?.className).toBe("plan-busy-text chat-wait");
  });
});
