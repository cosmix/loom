import { cleanup, render, screen, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { CHANGES_PATH } from "@/plan/review/changed-since";
import type { LeftBehind } from "./closing";
import { fixture } from "./transcript-test-kit";
import { LeftBehindCard, StartingCard, WrapUpCard } from "./wrap-up";

beforeEach(() => {
  vi.stubGlobal(
    "fetch",
    vi.fn(() => new Promise<Response>(() => {})),
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const view = (ui: ReactNode) => render(<MemoryRouter>{ui}</MemoryRouter>);
const summary = (fields: Partial<LeftBehind> = {}): LeftBehind => ({
  documents: [],
  decisions: [],
  questions: [],
  ...fields,
});

it("the wrap-up is no live region of its own, with its roundel, heading and one line per document", () => {
  const { container } = view(
    <WrapUpCard
      ticks={[
        { path: "doc/a.md", state: "done" },
        { path: "doc/b.md", state: "live" },
      ]}
    />,
  );

  const card = container.querySelector<HTMLElement>(".chat-wrap")!;
  expect(card.getAttribute("role")).toBeNull();
  expect(card.getAttribute("aria-live")).toBe("off");
  expect(within(card).getByRole("heading", { name: "Writing down what we decided" })).toBeTruthy();
  expect(card.textContent).toContain(
    "Putting this chat's decisions and open questions into the documents.",
  );
  expect(card.querySelectorAll(".chat-wrap-mark .aurora-roundel")).toHaveLength(1);
  const rows = [...container.querySelectorAll(".chat-wrap-ticks li")];
  expect(rows.map((row) => row.getAttribute("data-state"))).toEqual(["done", "live"]);
  expect(rows[0]!.querySelector(".chat-wrap-tick svg")).not.toBeNull();
  expect(rows[1]!.querySelector(".chat-wrap-tick svg")).toBeNull();
  expect(rows[0]!.querySelector(".chat-wrap-path")?.textContent).toBe("doc/a.md");
});

it("the wrap-up draws no list before the first document", () => {
  const { container } = view(<WrapUpCard ticks={[]} />);

  expect(container.querySelector(".chat-wrap-ticks")).toBeNull();
});

it("the left-behind card names its counts, documents, decisions and open questions", () => {
  const { container } = view(
    <LeftBehindCard
      summary={summary({
        documents: [
          { path: "doc/a.md", added: 9, removed: 2 },
          { path: "doc/b.md", added: null, removed: null },
        ],
        decisions: ["First", "Second"],
        questions: ["Open one"],
      })}
    />,
  );

  const card = screen.getByRole("region", { name: "Here's what this chat left behind" });
  const counts = [...card.querySelectorAll(".chat-left-behind-counts li")].map(
    (li) => li.textContent,
  );
  expect(counts).toEqual(["✓ 2 documents changed", "✓ 2 decisions", "? 1 open question"]);
  const docs = [...card.querySelectorAll(".chat-left-behind-docs li")].map((li) => li.textContent);
  expect(docs).toEqual(["doc/a.md +9 −2", "doc/b.md"]);
  // The number sits apart in its pill; the counts take the tool row's toned spans.
  expect(card.querySelector(".chat-left-behind-counts .chat-left-behind-n")?.textContent).toBe("2");
  expect(card.querySelector(".chat-left-behind-docs .chat-count-added")?.textContent).toBe("+9");
  expect(within(card).getByRole("link", { name: "See the changes" }).className).toBe(
    "chat-left-behind-link",
  );
  expect(within(card).getByRole("heading", { name: "Decided" })).toBeTruthy();
  expect(card.querySelectorAll("ol > li")).toHaveLength(2);
  expect(within(card).getByRole("heading", { name: "Still open" })).toBeTruthy();
  expect(container.querySelectorAll(".chat-left-behind-questions li")).toHaveLength(1);
  expect(within(card).getByRole("link", { name: "See the changes" }).getAttribute("href")).toBe(
    CHANGES_PATH,
  );
});

it("sections with nothing in them are not drawn", () => {
  const { container } = view(<LeftBehindCard summary={summary({ decisions: ["Only this"] })} />);

  expect(container.querySelector(".chat-left-behind-docs")).toBeNull();
  expect(container.querySelector(".chat-left-behind-questions")).toBeNull();
  expect(screen.queryByRole("heading", { name: "Still open" })).toBeNull();
  expect(screen.queryByRole("link", { name: "See the changes" })).toBeNull();
  expect([...container.querySelectorAll(".chat-left-behind-counts li")]).toHaveLength(1);
});

it("a chat that left nothing behind says so", () => {
  const { container } = view(<LeftBehindCard summary={summary()} />);

  expect(container.querySelector(".chat-left-behind-counts")).toBeNull();
  expect(container.textContent).toContain("Nothing was decided or changed.");
});

it("model and path text renders as text, never as markup", () => {
  const { container } = view(
    <>
      <WrapUpCard ticks={[{ path: "doc/<img src=x>.md", state: "live" }]} />
      <LeftBehindCard
        summary={summary({
          documents: [{ path: "doc/<b>x</b>.md", added: 1, removed: 0 }],
          decisions: ["<script>alert(1)</script>"],
          questions: ["<i>why</i>?"],
        })}
      />
    </>,
  );

  expect(container.querySelector("img, b, i:not([aria-hidden]), script")).toBeNull();
  expect(container.textContent).toContain("<script>alert(1)</script>");
  expect(container.textContent).toContain("<i>why</i>?");
});

it("the starting card holds one roundel and the starting brief", () => {
  const { container } = view(<StartingCard session={{ ...fixture.session!, state: "starting" }} />);

  expect(container.querySelectorAll(".chat-starting .aurora-roundel")).toHaveLength(1);
  expect(container.textContent).toContain("Starting");
});
