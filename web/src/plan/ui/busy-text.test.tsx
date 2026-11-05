import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { BusyText } from "./busy-text";

afterEach(cleanup);

describe("BusyText", () => {
  it("leads the words with a spinning roundel hidden from assistive technology", () => {
    const { container } = render(<BusyText>Starting the session</BusyText>);

    const root = container.firstElementChild!;
    expect(root.classList.contains("plan-busy-text")).toBe(true);
    const lead = root.firstElementChild!;
    expect(lead.getAttribute("aria-hidden")).toBe("true");
    expect(lead.querySelector(".aurora-roundel")?.getAttribute("data-busy")).toBe("true");
    expect(root.lastChild?.textContent).toBe("Starting the session");
    expect(screen.getByText("Starting the session")).toBe(root);
  });

  it("announces nothing of its own: the caller's words and role are the message", () => {
    render(<BusyText>Rebuilding</BusyText>);

    expect(screen.queryByRole("status")).toBeNull();
    expect(document.body.textContent).toBe("Rebuilding");
  });

  it("forwards a class name to its root", () => {
    const { container } = render(<BusyText className="chat-wait">Waiting</BusyText>);

    expect(container.firstElementChild?.className).toBe("plan-busy-text chat-wait");
  });
});
