import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import { Outline } from "./outline";

afterEach(cleanup);

describe("Outline", () => {
  it("renders bidi controls visibly in isolated heading titles", () => {
    const { container } = render(
      <MemoryRouter>
        <Outline
          path="doc/notes.md"
          sections={[
            {
              level: 2,
              title: "Review **safe**\u202e",
              anchor: "review",
              start_line: 1,
              end_line: 5,
            },
          ]}
        />
      </MemoryRouter>,
    );

    const link = screen.getByRole("link", { name: "Review safe⟨U+202E⟩" });
    const title = link.querySelector("span");
    expect(link.textContent).toBe("Review safe⟨U+202E⟩");
    expect(link.querySelector("strong")?.textContent).toBe("safe");
    expect(title?.style.unicodeBidi).toBe("isolate");
    expect(title?.style.direction).toBe("ltr");
    expect(container.textContent).not.toContain("\u202e");
  });

  it("renders inline heading formatting without backticks or nested links", () => {
    const { container } = render(
      <MemoryRouter>
        <Outline
          path="doc/notes.md"
          sections={[
            {
              level: 2,
              title: "8. New feature: the kickoff conversation `[D10, D11]`",
              anchor: "kickoff",
              start_line: 1,
              end_line: 5,
            },
            {
              level: 3,
              title: "**Read** *the* [guide](https://example.com)",
              anchor: "guide",
              start_line: 2,
              end_line: 5,
            },
          ]}
        />
      </MemoryRouter>,
    );

    const heading = screen.getByRole("link", {
      name: "8. New feature: the kickoff conversation [D10, D11]",
    });
    expect(heading.querySelector("code")?.textContent).toBe("[D10, D11]");
    expect(container.textContent).not.toContain("`");
    const child = screen.getByRole("link", { name: "Read the guide" });
    expect(child.querySelector("strong")?.textContent).toBe("Read");
    expect(child.querySelector("em")?.textContent).toBe("the");
    expect(screen.getAllByRole("link")).toHaveLength(2);
    expect(container.querySelector("a a")).toBeNull();
  });
});
