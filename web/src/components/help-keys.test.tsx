import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { HelpKeys } from "@/components/help-keys";

afterEach(cleanup);

const caps = (container: HTMLElement) =>
  [...container.querySelectorAll("kbd")].map((node) => node.textContent);

describe("HelpKeys", () => {
  it("renders arrows and alternatives as keycaps", () => {
    const { container } = render(<HelpKeys spec="↓ / j" />);
    expect(caps(container)).toEqual(["↓", "j"]);
    expect(container.querySelector(".help-gesture")).toBeNull();
  });

  it("renders Escape and a single letter as keycaps", () => {
    const { container } = render(<HelpKeys spec="Escape / ←" />);
    expect(caps(container)).toEqual(["Escape", "←"]);
    expect(caps(render(<HelpKeys spec="f" />).container)).toEqual(["f"]);
  });

  it("renders a key sequence as consecutive keycaps separated by then", () => {
    const { container } = render(<HelpKeys spec="g p" />);
    expect(caps(container)).toEqual(["g", "p"]);
    expect(container.textContent).toContain("then");
    expect(container.querySelector(".help-gesture")).toBeNull();
  });

  it("renders Ctrl + S and the command key chord as keycaps, not gestures", () => {
    const { container } = render(<HelpKeys spec="Ctrl + S / ⌘ + S" />);
    expect(caps(container)).toEqual(["Ctrl", "S", "⌘", "S"]);
    expect(container.querySelector(".help-gesture")).toBeNull();
    expect(container.textContent).not.toContain("then");
  });

  it("keeps a click gesture whose words are not keys as a gesture", () => {
    const { container } = render(<HelpKeys spec="Esc / Click empty canvas" />);
    expect(caps(container)).toEqual(["Esc"]);
    expect(container.querySelector(".help-gesture")?.textContent).toBe("Click empty canvas");
  });

  it("keeps a pointer gesture as a gesture", () => {
    const { container } = render(<HelpKeys spec="Drag a handle" />);
    expect(container.querySelector(".help-gesture")).not.toBeNull();
  });
});
