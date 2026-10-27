import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { parseAddress } from "@/plan/files/address";
import { fetchHeadDiff, fetchTurnDiff } from "@/plan/files/api";
import { openDocAtom, previousOpenTextAtom } from "@/plan/files/atoms";

import { DiffPage } from "./diff-page";
import { openDiff } from "./open-diff";

vi.mock("@/plan/files/api", () => ({ fetchHeadDiff: vi.fn(), fetchTurnDiff: vi.fn() }));
vi.mock("./diff-view", () => ({
  DiffView: (p: { before: string | null; after: string | null }) => (
    <div data-testid="diff">{`${p.before ?? "NULL"}|${p.after ?? "NULL"}`}</div>
  ),
}));

const PATH = "doc/product/section_01.md";
const diff = (before: string | null, after: string | null) => ({
  ok: true as const,
  value: { path: PATH, before, after },
});
const notFound = { ok: false as const, status: 404, error: "not_found", message: "gone" };

function mount(props: Parameters<typeof DiffPage>[0], store = createStore()) {
  return render(
    <Provider store={store}>
      <MemoryRouter>
        <DiffPage {...props} />
      </MemoryRouter>
    </Provider>,
  );
}

beforeEach(() => vi.mocked(fetchHeadDiff).mockReset().mockResolvedValue(diff("a", "b")));
afterEach(cleanup);

describe("DiffPage", () => {
  it("fetches the head diff for head and the turn diff for turn", async () => {
    mount({ path: PATH, kind: "head" });
    expect((await screen.findByTestId("diff")).textContent).toBe("a|b");
    expect(fetchHeadDiff).toHaveBeenCalledWith(PATH);
    cleanup();
    vi.mocked(fetchTurnDiff).mockResolvedValue(diff(null, "made"));
    mount({ path: PATH, kind: "turn", turn: "s1/t2" });
    expect((await screen.findByTestId("diff")).textContent).toBe("NULL|made");
    expect(fetchTurnDiff).toHaveBeenCalledWith("s1/t2", PATH);
  });

  it("shows a deleted file", async () => {
    vi.mocked(fetchHeadDiff).mockResolvedValue(diff("last", null));
    mount({ path: PATH, kind: "head" });
    expect((await screen.findByTestId("diff")).textContent).toBe("last|NULL");
  });

  it("explains a missing turn diff and links to the head diff", async () => {
    vi.mocked(fetchTurnDiff).mockResolvedValue(notFound);
    mount({ path: PATH, kind: "turn", turn: "s1/t2" });
    expect(await screen.findByText(/loom plan restarted/)).toBeTruthy();
    const link = screen.getByRole("link", { name: /uncommitted diff/i });
    expect(link.getAttribute("href")).toBe(openDiff("head", PATH));
  });

  it("says a head 404 means the document is gone", async () => {
    vi.mocked(fetchHeadDiff).mockResolvedValue(notFound);
    mount({ path: PATH, kind: "head" });
    expect(await screen.findByText("This document no longer exists")).toBeTruthy();
  });

  it("offers Retry on other failures", async () => {
    vi.mocked(fetchHeadDiff).mockResolvedValueOnce({
      ok: false,
      status: 500,
      error: "x",
      message: "boom",
    });
    mount({ path: PATH, kind: "head" });
    (await screen.findByRole("button", { name: "Retry" })).click();
    expect((await screen.findByTestId("diff")).textContent).toBe("a|b");
  });

  it("diffs the open document against its previous text", async () => {
    const store = createStore();
    store.set(previousOpenTextAtom, { path: PATH, text: "before" });
    store.set(openDocAtom, { doc: { path: PATH, text: "after" } as never, seenHash: "h" });
    mount({ path: PATH, kind: "open" }, store);
    await waitFor(() => expect(screen.getByTestId("diff").textContent).toBe("before|after"));
  });

  it("reports no change when nothing was open before", () => {
    mount({ path: PATH, kind: "open" });
    expect(screen.getByText("Nothing changed while open")).toBeTruthy();
  });

  it("round-trips a turn id with a slash through openDiff and parseAddress", () => {
    const url = new URL(openDiff("turn", PATH, "sess/t3"), "http://x");
    const address = parseAddress({ pathname: url.pathname, search: url.search, hash: "" });
    expect(address).toMatchObject({ path: PATH, diff: "turn", turn: "sess/t3" });
  });
});
