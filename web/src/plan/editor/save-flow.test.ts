import { describe, expect, it } from "vitest";
import invalidFixture from "@/plan/api/fixtures/files-save-invalid.json";
import { saveResultSchema } from "@/plan/api/schema-files";
import { initialSaveFlow, saveFlow } from "./save-flow";

describe("saveFlow", () => {
  it("keeps the latest merge text and updates the base on each conflict", () => {
    let state = initialSaveFlow("original", "h1");
    state = saveFlow(state, { type: "change", text: "my edit" });
    state = saveFlow(state, { type: "save" });
    state = saveFlow(state, {
      type: "result",
      result: {
        ok: true,
        value: {
          result: "conflict",
          current_text: "disk one",
          current_hash: "h2",
        },
      },
    });
    expect(state).toMatchObject({
      status: "conflict",
      currentText: "disk one",
      text: "my edit",
      openedHash: "h2",
      clearDrafts: false,
    });
    state = saveFlow(state, { type: "change", text: "merged edit" });
    state = saveFlow(state, { type: "save" });
    state = saveFlow(state, {
      type: "result",
      result: {
        ok: true,
        value: {
          result: "conflict",
          current_text: "disk two",
          current_hash: "h3",
        },
      },
    });
    expect(state).toMatchObject({
      status: "conflict",
      currentText: "disk two",
      text: "merged edit",
      openedHash: "h3",
      clearDrafts: false,
    });
    state = saveFlow(state, { type: "save" });
    state = saveFlow(state, {
      type: "result",
      result: {
        ok: true,
        value: {
          result: "saved",
          hash: "h4",
          validation: null,
        },
      },
    });
    expect(state).toMatchObject({
      status: "saved",
      savedText: "merged edit",
      openedHash: "h4",
      currentText: null,
      clearDrafts: true,
    });
  });

  it("retains drafts and validation or server errors when a save fails", () => {
    let state = saveFlow(initialSaveFlow("original", "h1"), { type: "change", text: "edit" });
    state = saveFlow(state, { type: "save" });
    const invalid = saveResultSchema.parse(invalidFixture);
    state = saveFlow(state, { type: "result", result: { ok: true, value: invalid } });
    expect(state).toMatchObject({
      status: "invalid",
      validation: invalidFixture.validation,
      text: "edit",
      clearDrafts: false,
    });
    state = saveFlow(state, { type: "save" });
    state = saveFlow(state, {
      type: "result",
      result: { ok: false, status: 403, error: "held", message: "Another browser holds authoring" },
    });
    expect(state).toMatchObject({
      status: "error",
      error: "Another browser holds authoring",
      text: "edit",
      clearDrafts: false,
    });
    state = saveFlow(state, { type: "discard" });
    expect(state).toMatchObject({ status: "idle", text: "original", clearDrafts: true });
  });

  it("does not clear newer edits made while a save is in flight", () => {
    let state = saveFlow(initialSaveFlow("original", "h1"), { type: "change", text: "submitted" });
    state = saveFlow(state, { type: "save" });
    state = saveFlow(state, { type: "change", text: "newer edit" });
    state = saveFlow(state, {
      type: "result",
      result: {
        ok: true,
        value: {
          result: "saved",
          hash: "h2",
          validation: null,
        },
      },
    });
    expect(state).toMatchObject({
      openedHash: "h2",
      savedText: "submitted",
      text: "newer edit",
      clearDrafts: false,
    });
  });

  it("opens a merge for an older draft without clearing it", () => {
    const state = saveFlow(initialSaveFlow("current", "h2"), {
      type: "merge",
      currentText: "current",
      currentHash: "h2",
      text: "draft",
    });
    expect(state).toMatchObject({
      status: "conflict",
      currentText: "current",
      text: "draft",
      openedHash: "h2",
      clearDrafts: false,
    });
  });
});
