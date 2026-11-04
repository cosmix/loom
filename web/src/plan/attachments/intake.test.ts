import { createStore } from "jotai/vanilla";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { addAttachmentAtom, pendingAttachmentsAtom } from "./atoms";
import { forgetPreview, previewUrl, rememberPreview } from "./intake";

const revoke = vi.fn();
const image = () => new File(["x"], "a.png", { type: "image/png" });

beforeEach(() => {
  vi.stubGlobal(
    "URL",
    Object.assign(URL, { createObjectURL: () => "blob:shared", revokeObjectURL: revoke }),
  );
});
afterEach(() => {
  forgetPreview("same");
  forgetPreview("same");
  revoke.mockClear();
  vi.unstubAllGlobals();
});

it("a thumbnail shared by two chips is revoked only when the last one is removed", () => {
  rememberPreview("same", image());
  rememberPreview("same", image());

  forgetPreview("same");
  expect(revoke).not.toHaveBeenCalled();
  expect(previewUrl("same")).toBe("blob:shared");

  forgetPreview("same");
  expect(revoke).toHaveBeenCalledExactlyOnceWith("blob:shared");
  expect(previewUrl("same")).toBeNull();
});

it("a file that is not an image gets no thumbnail", () => {
  rememberPreview("same", new File(["x"], "a.md", { type: "text/markdown" }));
  expect(previewUrl("same")).toBeNull();
});

it("a duplicate file dropped by the chip list gives its hold back", () => {
  const store = createStore();
  const chip = {
    kind: "file",
    id: "same",
    name: "a.png",
    media_type: "image/png",
    bytes: 1,
  } as const;
  rememberPreview("same", image());
  store.set(addAttachmentAtom, chip);
  rememberPreview("same", image());
  store.set(addAttachmentAtom, chip);
  expect(store.get(pendingAttachmentsAtom)).toHaveLength(1);

  forgetPreview("same");

  expect(revoke).toHaveBeenCalledExactlyOnceWith("blob:shared");
  expect(previewUrl("same")).toBeNull();
});
