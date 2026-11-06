import { cleanup, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { authorWithOffers, doc, empty, makeStubFetch, renderAt } from "./concept-route-support";

const extras = vi.hoisted(() => ({
  header: vi.fn(() => null),
  chip: vi.fn(() => null),
  action: vi.fn(() => null),
}));
vi.mock("./page-header-extras", () => ({
  PageHeaderExtras: extras.header,
  SummaryAction: extras.action,
  SummaryDetail: () => null,
}));
vi.mock("./key-chip-extras", () => ({ KeyChipExtras: extras.chip }));

let pages: Record<string, unknown> = {};
let docs: Record<string, unknown> = {};
let failing = false;
const fetched: string[] = [];

beforeEach(() => {
  pages = {};
  docs = {};
  failing = false;
  fetched.length = 0;
  extras.header.mockClear();
  extras.chip.mockClear();
  extras.action.mockClear();
  window.sessionStorage.clear();
  vi.stubGlobal("fetch", vi.fn(makeStubFetch(() => ({ pages, docs, failing }), fetched)));
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("No metaplan yet", async () => {
  pages = {
    "overview/demo/engineering": {
      ...empty("no_metaplan", { offer: "start-metaplan" }),
      key: "overview/demo/engineering",
    },
  };
  renderAt("/f/demo/engineering", authorWithOffers);
  expect(await screen.findByText("No metaplan yet")).toBeTruthy();
  expect(screen.getByRole("button", { name: /metaplan/ })).toBeTruthy();
});
it("plans without metaplan list and offer", async () => {
  pages = {
    "overview/demo/engineering": {
      ...empty("plans_no_metaplan", {
        docs: [doc("a", "Alpha", "demo/engineering/a")],
        offer: "start-metaplan",
      }),
      key: "overview/demo/engineering",
    },
  };
  renderAt("/f/demo/engineering", authorWithOffers);
  const list = await screen.findByRole("list", { name: "Documents" });
  expect(within(list).getByRole("link", { name: "Alpha" })).toBeTruthy();
  expect(screen.getByRole("button", { name: /metaplan/ })).toBeTruthy();
});
