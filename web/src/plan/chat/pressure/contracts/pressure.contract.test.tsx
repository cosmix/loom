import { cleanup, render } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it } from "vitest";

import { codexStatusSchema } from "@/plan/api/schema-pressure";

import * as m from "../codex-status";

const TAIL = "<img src=x onerror=alert(1)>";

afterEach(() => {
  cleanup();
});

// One top-level it() carrying the whole contract name: vitest 5 joins describe and it with
// " > ", so loom's space-joined -t filter would select nothing under a describe block.
it("pressure header renders a codex log tail as text", () => {
  const status = codexStatusSchema.parse({
    model: "gpt-6.1-terra",
    state: "failed",
    report_path: null,
    log_tail: TAIL,
  });

  render(
    <Provider store={createStore()}>
      <MemoryRouter>
        <m.CodexStatusItem status={status} />
      </MemoryRouter>
    </Provider>,
  );

  expect(document.body.textContent).toContain(TAIL);
  expect(document.body.querySelector("img")).toBeNull();
});
