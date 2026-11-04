import { expect, it } from "vitest";

import type { AttachmentDraft } from "@/plan/attachments";
import { chipLabel } from "./chips";
import { toAttachmentInput } from "./commands";
import { installPanelLifecycle } from "./panel-test-kit";
import { item, mount } from "./transcript-test-kit";

// Wide matchMedia, cleanup, mock restore and global unstub per test.
installPanelLifecycle();

function summary(text: string): Extract<AttachmentDraft, { kind: "summary" }> {
  return {
    kind: "summary",
    summary: "demo/product/section_01",
    sentence: "sentence-1",
    text,
    restsOn: [],
    hash: null,
  };
}

it("an assistant reply hides its loom-kickoff block", () => {
  const fence = "`".repeat(3);
  const markdown =
    "I read these first.\n\n" +
    fence +
    'loom-kickoff\n{"problem":"p","decisions":[],"questions":["Which hosts?"]}\n' +
    fence +
    "\n";
  const { log } = mount([item("text", { markdown })]);
  const shown = log.textContent ?? "";
  expect(shown).toContain("I read these first.");
  expect(shown).not.toContain("loom-kickoff");
  expect(shown).not.toContain("Which hosts?");
  expect(shown).not.toContain('"questions"');
});

it("two selections on one summary page get different chips", () => {
  const first = chipLabel(toAttachmentInput(summary("Alpha holds the first idea of the section.")));
  const second = chipLabel(toAttachmentInput(summary("Beta holds a second, different idea.")));
  expect(first).not.toBe(second);
  expect(first).toContain("Alpha holds the first idea");
  expect(second).toContain("Beta holds a second");
});

it("a file attachment is sent by its id", () => {
  const draft = {
    kind: "file",
    id: "0123456789abcdef",
    name: "sketch.png",
    media_type: "image/png",
    bytes: 1234,
  } as unknown as AttachmentDraft;
  const input: Record<string, unknown> = { ...toAttachmentInput(draft) };
  expect(input).toEqual({ kind: "file", id: "0123456789abcdef", name: "sketch.png" });
  expect("path" in input).toBe(false);
  expect("quote" in input).toBe(false);
});
