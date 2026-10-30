import { fireEvent, waitFor, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";

import { cite, sent } from "./reading-column-fixtures";
import {
  button,
  group,
  installColumnTestHooks,
  mark,
  mountProduct,
} from "./reading-column-harness";

installColumnTestHooks();

const requested = () =>
  vi
    .mocked(fetch)
    .mock.calls.map(([input]) => String(input))
    .filter((url) => /concept/.test(url));

it("prefetches the summary pages the overview names and no others", async () => {
  mountProduct([
    sent("a", "Planning opens.", [cite("section_01", "x")], [mark("section_01", "Planning", 1)]),
    // `plans/gone` is a plan deleted since the overview was committed: no key chip or mark names it.
    sent("b", "Gone opens.", [cite("plans/gone", "y")]),
  ]);

  await waitFor(() =>
    expect(requested().some((url) => url.endsWith("demo/product/section_01"))).toBe(true),
  );
  expect(requested().some((url) => url.includes("plans/gone"))).toBe(false);
});

it("says a cited summary the page cannot name no longer exists, without asking the server", async () => {
  const { column } = mountProduct([sent("b", "Gone opens.", [cite("plans/gone", "y")])]);

  fireEvent.click(button(column, /^Gone opens/));
  const run = await group(column);

  expect(await within(run).findByText(/summary no longer exists/)).toBeTruthy();
  expect(within(run).queryByText("Loading")).toBeNull();
  expect(requested().some((url) => url.includes("plans/gone"))).toBe(false);
});
