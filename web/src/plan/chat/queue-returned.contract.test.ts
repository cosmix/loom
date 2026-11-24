import { createStore } from "jotai/vanilla";
import { describe, expect, it } from "vitest";

import { chatFrameSchema } from "@/plan/api/schema-chat";
import { applyChatFrame, queueReturnedAtom } from "@/plan/chat/model";

describe("queue returned contract", () => {
  it("a recovery frame refills the composer without an interrupt intent", () => {
    for (const cause of ["recovery", "pressure"]) {
      const frame = chatFrameSchema.parse({ frame: "queue_returned", texts: ["Lost"], cause });
      const store = createStore();
      applyChatFrame(store, frame);
      expect(store.get(queueReturnedAtom), cause).toEqual(["Lost"]);
    }

    const unsolicited = chatFrameSchema.parse({ frame: "queue_returned", texts: ["Lost"] });
    const store = createStore();
    applyChatFrame(store, unsolicited);
    expect(store.get(queueReturnedAtom)).toEqual([]);
  });
});
