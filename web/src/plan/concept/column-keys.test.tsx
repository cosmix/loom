import { waitFor } from "@testing-library/react";
import { expect, it } from "vitest";

import { sent, src } from "./reading-column-fixtures";
import { active, installColumnTestHooks, mountProduct, press } from "./reading-column-harness";

installColumnTestHooks();

it.each([
  ["doc/product/demo/section_02.md", "/f/demo/source/product/demo/section_02.md#intro"],
  ["doc/product/section_02.md", "/files/product/section_02.md#intro"],
])("f opens the source of %s at %s", async (path, address) => {
  const { column, router } = mountProduct([sent("a", "Targets here.", [src(path, "intro")])]);
  column.focus();
  await waitFor(() => expect(active().textContent).toMatch(/^Targets/));
  press("f");
  expect(router.state.location.pathname + router.state.location.hash).toBe(address);
});
