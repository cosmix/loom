import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Provider } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import { featureOfKey } from "@/plan/api/page-key";
import { conceptPageSchema, type ConceptPage } from "@/plan/api/schema-concept";
import type { DocView } from "@/plan/api/schema-files";
import { pendingAttachmentsAtom } from "@/plan/attachments";
import { focusComposerAtom } from "@/plan/chat/composer-hooks";
import { authorStore, makeDoc } from "@/plan/files/doc-heading-kit";

import { PageHeading } from "./page-heading";

const kept = vi.hoisted(() => ({ doc: null as DocView | null, failed: false }));
vi.mock("./open-all", () => ({ useOpenAll: () => vi.fn() }));
vi.mock("./passage-doc", () => ({ useKeptDoc: () => ({ doc: kept.doc, failed: kept.failed }) }));
vi.mock("./page-header-extras", () => ({ PageHeaderExtras: () => null }));

afterEach(() => {
  cleanup();
  kept.failed = false;
});

function pageWith(overrides: Partial<ConceptPage> = {}): ConceptPage {
  return conceptPageSchema.parse({
    ...structuredClone(summaryFixture),
    state: null,
    implemented_by: [],
    feature: featureOfKey(summaryFixture.key),
    ...overrides,
  });
}

function showWithDoc(
  doc: DocView | null,
  overrides: Partial<ConceptPage> = {},
  store = authorStore(),
) {
  kept.doc = doc;
  render(<PageHeading page={pageWith(overrides)} />, {
    wrapper: ({ children }) => (
      <Provider store={store}>
        <MemoryRouter>{children}</MemoryRouter>
      </Provider>
    ),
  });
  return store;
}

it("offers Edit into file mode's editor and Discuss over the source, beside Open all", () => {
  const doc = makeDoc({ editable: true, lock: null });
  const store = showWithDoc(doc);

  const edit = screen.getByRole("link", { name: "Edit" });
  expect(edit.getAttribute("href")).toBe(`/files/${doc.path.replace(/^doc\//, "")}?edit=1`);
  fireEvent.click(screen.getByRole("button", { name: "Discuss" }));
  expect(store.get(pendingAttachmentsAtom)).toMatchObject([
    { kind: "source", path: doc.path, hash: doc.hash },
  ]);
  expect(store.get(focusComposerAtom)).toBe(true);
  expect(screen.getByRole("button", { name: "Open all" })).toBeTruthy();
});

it("disables Edit with the lock and file mode's title when the source is locked", () => {
  showWithDoc(makeDoc({ lock: { reason: "running", notice: "Running.", plan: null } }));

  const edit = screen.getByRole("button", { name: "Edit" });
  expect(edit.getAttribute("aria-disabled")).toBe("true");
  expect(edit.getAttribute("title")).toBe("Read-only while running");
  expect(edit.querySelector("svg.lucide-lock")).not.toBeNull();
});

it("keeps Edit and Discuss disabled until the source loads and hides both for a reader", () => {
  showWithDoc(null);
  expect((screen.getByRole("button", { name: "Edit" }) as HTMLButtonElement).disabled).toBe(true);
  const discuss = screen.getByRole("button", { name: "Discuss" }) as HTMLButtonElement;
  expect(discuss.disabled).toBe(true);
  cleanup();

  showWithDoc(makeDoc({ editable: true, lock: null }), {}, authorStore(false));
  expect(screen.queryByRole("link", { name: "Edit" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Discuss" })).toBeNull();
});

it("renders neither Edit nor Discuss without a source path, and no Open all over an empty page", () => {
  showWithDoc(makeDoc(), {
    path: null,
    empty: { kind: "no_vision", count: null, offer: null, docs: [] },
  });

  expect(screen.queryByRole("button", { name: "Edit" })).toBeNull();
  expect(screen.queryByRole("link", { name: "Edit" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Discuss" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Open all" })).toBeNull();
});

it("renders neither Edit nor Discuss when the source failed to load, and keeps Open all", () => {
  kept.failed = true;
  showWithDoc(null);

  expect(screen.queryByRole("button", { name: "Edit" })).toBeNull();
  expect(screen.queryByRole("link", { name: "Edit" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Discuss" })).toBeNull();
  expect(screen.getByRole("button", { name: "Open all" })).toBeTruthy();
});
