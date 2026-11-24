import { afterEach, beforeEach, describe, expect, it } from "vitest";

import * as drafts from "@/plan/editor/drafts";
import { serverInfoAtom } from "@/plan/state/atoms";
import { store } from "@/state/store";

// The editor id parameters are new; these views keep the file type-correct before they exist.
const saveDraft = drafts.saveDraft as (
  docPath: string,
  openedHash: string,
  text: string,
  editorId?: string,
) => drafts.SaveDraftResult;
const clearDrafts = drafts.clearDrafts as (path: string, editorId?: string) => void;

beforeEach(() => {
  localStorage.clear();
  store.set(serverInfoAtom, {
    project: "contract",
    project_key: "contract",
    port: 7272,
    remote: false,
    read_only: [],
    stopping: false,
    you: { public_id: "aaaaaaaaaaaaaaaa", name: "Ada" },
  });
});

afterEach(() => {
  localStorage.clear();
  store.set(serverInfoAtom, null);
});

describe("draft storage contract", () => {
  it("two tabs keep separate recovery copies and a discard clears only its own", () => {
    expect(saveDraft("a.md", "h1", "tab one", "tab-1")).toBe("saved");
    expect(saveDraft("a.md", "h1", "tab two", "tab-2")).toBe("saved");

    const both = drafts.findDrafts("a.md");
    expect(both).toHaveLength(2);
    expect(both.map((draft) => draft.text).sort()).toEqual(["tab one", "tab two"]);

    clearDrafts("a.md", "tab-1");
    const left = drafts.findDrafts("a.md");
    expect(left).toHaveLength(1);
    expect(left[0]).toMatchObject({ openedHash: "h1", text: "tab two" });
  });
});
