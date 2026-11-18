import { cleanup, render } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter, useLocation, type InitialEntry } from "react-router";
import { afterEach, beforeEach, vi } from "vitest";

import treeFixture from "@/plan/api/fixtures/files-tree.json";
import { fileTreeSchema, type FileTree } from "@/plan/api/schema-files";
import { fetchDoc, fetchTree } from "@/plan/files/api";
import { treeAtom } from "@/plan/files/atoms";
import { FileMode } from "@/plan/files/file-mode";

export { fetchDoc };

export const scrolled = vi.fn();
export const tree = (): FileTree => fileTreeSchema.parse(structuredClone(treeFixture));

export function setWide(wide: boolean) {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: (query: string) => ({
      matches: wide,
      media: query,
      addEventListener() {},
      removeEventListener() {},
    }),
  });
}

function Where() {
  const { pathname, search } = useLocation();
  return <output data-testid="where">{pathname + search}</output>;
}

export function renderMode(
  entry: InitialEntry,
  fileTree: FileTree = tree(),
  store = createStore(),
) {
  store.set(treeAtom, fileTree);
  render(
    <Provider store={store}>
      <MemoryRouter initialEntries={[entry]}>
        <FileMode />
        <Where />
      </MemoryRouter>
    </Provider>,
  );
  return store;
}

beforeEach(() => {
  setWide(true);
  scrolled.mockReset();
  Element.prototype.scrollIntoView = scrolled;
  vi.mocked(fetchTree).mockReset();
  vi.mocked(fetchDoc).mockReset();
});
afterEach(cleanup);
