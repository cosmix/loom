import { act, cleanup, fireEvent, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import type { Attachment, Delivery } from "@/plan/api/schema-chat";
import type { connectPlanSocket } from "@/plan/api/socket";
import { authoringAtom, connectionAtom, planSocketAtom, youAtom } from "@/plan/state/atoms";
import {
  applyChatFrame,
  CONNECTION_LOST,
  registerPending,
  rejectAllPending,
  snapshotAtom,
  unconfirmedSendsAtom,
} from "./model";
import { item, mount } from "./transcript-test-kit";
import { toWire } from "./wire-input-test-kit";

const me = { public_id: "me", name: "Me" };
const attachment: Attachment = {
  kind: "source",
  path: "doc/product/note.md",
  anchor: null,
  lines: null,
  stage: null,
  summary_sentence: null,
  rests_on: [],
  quote: "Selected passage",
  changed_since_selected: true,
  locked: "done",
};
// A lost entry keeps the composer's own input; an item has no real hash left, so the server's
// "changed since selected" flag is carried by a hash no document can have.
const entryWire = toWire(attachment, "hash-at-selection");
const flaggedWire = toWire(attachment, "changed-since-selected");
const unflaggedWire = toWire(attachment);

type Harness = ReturnType<typeof mount> & { sent: Record<string, unknown>[] };

/**
 * Mounts a "sent" user item, then moves it to `delivery` through an item_replaced frame. With
 * `lost`, the connection dropped before the result, so the send is also kept as unconfirmed.
 */
function arrive(
  delivery: Delivery,
  holder = true,
  lost = false,
  attached: Attachment = attachment,
): Harness {
  const user = item("user", {
    origin: null,
    attachments: [attached],
    text: "Lost message",
    delivery: "sent",
  });
  const h = mount([user]);
  const sent: Record<string, unknown>[] = [];
  const socket = {
    send: (message: Record<string, unknown>) => (sent.push(message), true),
    subscribe: () => () => {},
    onResync: () => () => {},
    dispose: () => {},
  };
  act(() => {
    h.store.set(youAtom, me);
    h.store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: null });
    h.store.set(authoringAtom, {
      holder: { ...me, away_since: null, frees_at: null, public_id: holder ? "me" : "ada" },
    });
    h.store.set(planSocketAtom, socket as unknown as ReturnType<typeof connectPlanSocket>);
    if (lost) {
      registerPending(h.store, "lost", { text: user.text, attachments: [entryWire] }).catch(
        () => {},
      );
      rejectAllPending(h.store, CONNECTION_LOST, true);
    }
    applyChatFrame(h.store, {
      frame: "delta",
      op: "item_replaced",
      turn_id: h.store.get(snapshotAtom).turns[0]!.id,
      item: { ...user, delivery },
    });
  });
  return { ...h, sent };
}

const accept = (h: Harness) =>
  act(async () =>
    applyChatFrame(h.store, {
      frame: "result",
      client_id: h.sent[0]!.client_id as string,
      ok: true,
      error: null,
    }),
  );

beforeEach(() => {
  vi.stubGlobal("fetch", vi.fn());
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("sent message shows a muted Sending", () => {
  const { log } = arrive("sent");
  // `.plan-fine` is the muted fine-print style (ui/design.css: color: var(--muted-foreground)).
  expect(within(log).getByText("Sending").closest(".plan-fine")).not.toBeNull();
  expect(within(log).queryByText("Not delivered")).toBeNull();
  expect(within(log).queryByRole("button", { name: "Resend" })).toBeNull();
});

it("delivered message carries no marker", () => {
  const { log } = arrive("delivered");
  expect(within(log).getByText("Lost message")).toBeTruthy();
  expect(within(log).queryByText("Sending")).toBeNull();
  expect(within(log).queryByText("Not delivered")).toBeNull();
  expect(within(log).queryByRole("button", { name: "Resend" })).toBeNull();
});

it("an undelivered item the bridge marks after an accepted send offers Resend with its attachments", async () => {
  const h = arrive("undelivered");
  expect(h.store.get(unconfirmedSendsAtom)).toEqual([]);
  const marker = within(h.log).getByText("Not delivered");
  // The tone colours the glyph only; the words and what holds them stay in the text colour.
  expect(marker.querySelector('[aria-hidden="true"]')?.className).toContain("tone-warning");
  expect(marker.closest('[class*="tone-warning"]')).toBeNull();
  fireEvent.click(within(h.log).getByRole("button", { name: "Resend" }));
  expect(h.sent[0]).toMatchObject({
    command: "send",
    text: "Lost message",
    attachments: [flaggedWire],
  });
  await accept(h);
  await waitFor(() => expect(within(h.log).queryByRole("button", { name: "Resend" })).toBeNull());
});

it("an undelivered item with a lost-connection entry resends the entry and clears it", async () => {
  const h = arrive("undelivered", true, true);
  expect(h.store.get(unconfirmedSendsAtom)).toHaveLength(1);
  fireEvent.click(within(h.log).getByRole("button", { name: "Resend" }));
  expect(h.sent[0]).toMatchObject({
    command: "send",
    text: "Lost message",
    attachments: [entryWire],
  });
  await accept(h);
  expect(h.store.get(unconfirmedSendsAtom)).toEqual([]);
  await waitFor(() => expect(within(h.log).queryByRole("button", { name: "Resend" })).toBeNull());
});

it("resend is held back from viewers", () => {
  const viewer = arrive("undelivered", false);
  expect(within(viewer.log).getByText("Not delivered")).toBeTruthy();
  expect(within(viewer.log).queryByRole("button", { name: "Resend" })).toBeNull();
});

it("an attachment that had not changed resends without a selected hash", () => {
  const h = arrive("undelivered", true, false, { ...attachment, changed_since_selected: false });
  fireEvent.click(within(h.log).getByRole("button", { name: "Resend" }));
  expect(h.sent[0]).toMatchObject({ attachments: [unflaggedWire] });
});
