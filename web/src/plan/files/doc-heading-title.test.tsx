import { cleanup, screen, within } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";

import { makeDoc, mount } from "./doc-heading-kit";

afterEach(cleanup);

const METAPLAN = {
  kind: "metaplan",
  plan: null,
  title: "`loom plan` Engineering Metaplan",
} as const;

it("draws a document title's code spans as code, not as backticks", () => {
  mount(makeDoc(METAPLAN));

  const title = screen.getByRole("heading", { level: 1 });
  expect(title.textContent).toBe("loom plan Engineering Metaplan");
  expect(title.querySelector("code")?.textContent).toBe("loom plan");
});

it("sets the breadcrumb's name for the document without the title's backticks", () => {
  mount(makeDoc(METAPLAN));

  const nav = screen.getByRole("navigation", { name: "Breadcrumb" });
  expect(within(nav).getByText("loom plan Engineering Metaplan")).toBeTruthy();
  expect(nav.textContent).not.toContain("`");
});

it("keeps a title's bidirectional override as visible text inside an isolated run", () => {
  mount(makeDoc({ ...METAPLAN, title: "Round‮ two" }));

  const title = screen.getByRole("heading", { level: 1 });
  expect(title.textContent).toBe("Round⟨U+202E⟩ two");
  expect((title.firstElementChild as HTMLElement).style.unicodeBidi).toBe("isolate");
});
