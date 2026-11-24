import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { docChangedAtom } from "@/plan/files/atoms";
import { store } from "@/state/store";

import { findDrafts, loadDraft, saveDraft } from "./drafts";
import { doc, renderEdit, setupEditMode, source } from "./edit-mode-kit";

vi.mock("@/plan/editor/load", () => ({
  loadCodeMirror: async () => (await import("./edit-mode-kit")).fake,
}));

setupEditMode();

describe("EditMode draft recovery", () => {
  it("writes one draft after typing pauses instead of one per keystroke", async () => {
    renderEdit();
    const editor = await source();
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    for (const value of ["a", "ab", "abc"]) fireEvent.input(editor, { target: { value } });
    expect(setItem).not.toHaveBeenCalled();
    await waitFor(() => expect(loadDraft(doc.path, doc.hash)).toBe("abc"), { timeout: 2000 });
    expect(setItem).toHaveBeenCalledTimes(1);
  });

  it("keeps the pending draft when the editor goes away", async () => {
    renderEdit();
    fireEvent.input(await source(), { target: { value: "kept" } });
    cleanup();
    expect(loadDraft(doc.path, doc.hash)).toBe("kept");
  });

  it("does not bring a discarded draft back", async () => {
    const router = renderEdit();
    fireEvent.input(await source(), { target: { value: "unsaved" } });
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    fireEvent.click(await screen.findByRole("button", { name: "Discard" }));
    await waitFor(() => expect(router.state.location.search).toBe(""));
    await new Promise((resolve) => setTimeout(resolve, 700));
    expect(findDrafts(doc.path)).toEqual([]);
  });

  // The earlier load's copy is the newest, so it is the one startState adopts; the other
  // open tab's copy belongs to a tab still running and must survive.
  function seedOtherEditors(hash: string, text: string) {
    const now = vi.spyOn(Date, "now");
    now.mockReturnValue(1);
    saveDraft(doc.path, hash, "other open tab", "open-tab");
    now.mockReturnValue(2);
    saveDraft(doc.path, hash, text, "earlier-load");
    now.mockRestore();
  }
  const editors = () => findDrafts(doc.path).map((draft) => draft.editorId);

  it("restores a draft an earlier page load wrote and removes it on Discard", async () => {
    seedOtherEditors(doc.hash, "from before reload");
    const router = renderEdit();
    expect((await source()).value).toBe("from before reload");
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    fireEvent.click(await screen.findByRole("button", { name: "Discard" }));
    await waitFor(() => expect(router.state.location.search).toBe(""));
    expect(editors()).toEqual(["open-tab"]);
  });

  it("removes the merged earlier-load draft once its text is saved", async () => {
    seedOtherEditors("sha256:older", "stale text");
    const saved = { result: "saved", hash: "sha256:new", validation: null };
    vi.stubGlobal(
      "fetch",
      vi.fn(() => Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve(saved) })),
    );
    renderEdit();
    const right = (await screen.findByTestId("merge-right")) as HTMLTextAreaElement;
    expect(right.value).toBe("stale text");
    fireEvent.keyDown(right, { key: "s", ctrlKey: true });
    await waitFor(() => expect(store.get(docChangedAtom)[doc.path]).toBe("sha256:new"));
    await waitFor(() => expect(editors()).toEqual(["open-tab"]));
  });

  // The first own draft write takes over the adopted copy. Without that, a chain of reloads
  // leaves the oldest copy behind and a later load restores text the user discarded.
  it("hands the adopted draft over on the first own write so a reload chain cannot revive it", async () => {
    seedOtherEditors(doc.hash, "typed in load one");
    renderEdit();
    const editor = await source();
    expect(editors()).toEqual(["earlier-load", "open-tab"]);
    fireEvent.input(editor, { target: { value: "typed in load two" } });
    await waitFor(() => expect(editors()).not.toContain("earlier-load"), { timeout: 2000 });
    expect(editors()).toContain("open-tab");
    expect(findDrafts(doc.path).find((draft) => draft.editorId !== "open-tab")?.text).toBe(
      "typed in load two",
    );

    cleanup();
    const router = renderEdit();
    expect((await source()).value).toBe("typed in load two");
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    fireEvent.click(await screen.findByRole("button", { name: "Discard" }));
    await waitFor(() => expect(router.state.location.search).toBe(""));
    expect(editors()).toEqual(["open-tab"]);
  });

  // An in-app remount adopts the copy this page load wrote itself, so the adopted id is the
  // load's own id. The clear must run before the write, or it deletes the copy just written.
  it("keeps the fresh copy when the adopted editor id is this load's own id", async () => {
    renderEdit();
    fireEvent.input(await source(), { target: { value: "first text" } });
    cleanup();
    expect(findDrafts(doc.path).map((draft) => draft.text)).toEqual(["first text"]);

    renderEdit();
    const editor = await source();
    expect(editor.value).toBe("first text");
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    fireEvent.input(editor, { target: { value: "second text" } });
    await waitFor(() => expect(setItem).toHaveBeenCalled(), { timeout: 2000 });
    expect(findDrafts(doc.path).map((draft) => draft.text)).toEqual(["second text"]);
  });

  it("keeps a still-open tab's newer copy of the adopted editor id on Discard", async () => {
    seedOtherEditors(doc.hash, "typed in load one");
    const router = renderEdit();
    fireEvent.input(await source(), { target: { value: "typed in load two" } });
    await waitFor(() => expect(editors()).not.toContain("earlier-load"), { timeout: 2000 });
    saveDraft(doc.path, doc.hash, "still typing elsewhere", "earlier-load");

    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    fireEvent.click(await screen.findByRole("button", { name: "Discard" }));
    await waitFor(() => expect(router.state.location.search).toBe(""));
    expect(editors().sort()).toEqual(["earlier-load", "open-tab"]);
  });
});
