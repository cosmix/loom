import { act, fireEvent, screen } from "@testing-library/react";
import { expect, it } from "vitest";

import { EmptyOffer } from "@/plan/concept/empty-offer";
import { checklistAtom } from "@/plan/state/atoms";
import { snapshotAtom } from "./model";
import { createOfferStarter } from "./offer-flow";
import { registerAuthoringOffers } from "./offer-handlers";
import {
  disposers,
  harness,
  installStartChatLifecycle,
  offer,
  session,
  wroteTurns,
} from "./start-chat-test-kit";
import { NewChatButton } from "./start-chat";

installStartChatLifecycle();

it("a refused end keeps its reason after the chat has ended meanwhile", async () => {
  const current = { ...session, state: "idle" as const };
  const h = harness(current);
  h.mount(<NewChatButton variant="head" />);
  fireEvent.click(screen.getByRole("button", { name: "New chat" }));
  h.result(0, "Chat is busy");
  act(() =>
    h.store.set(snapshotAtom, {
      session: { ...current, state: "ended" },
      turns: wroteTurns,
      queue: [],
      previous: null,
    }),
  );
  expect(await screen.findByText("Chat is busy")).toBeTruthy();
});

it.each([
  ["the user never wrote to", { ...session, state: "idle" as const }, []],
  ["is already ending", { ...session, state: "ending" as const }, wroteTurns],
])("New chat asks nothing when the chat %s", (_name, current, turns) => {
  const h = harness(current, turns);
  h.mount(<NewChatButton variant="head" />);
  fireEvent.click(screen.getByRole("button", { name: "New chat" }));
  expect(window.confirm).not.toHaveBeenCalled();
});

it("an offer asks nothing when the chat the user never wrote to is ended", () => {
  const h = harness({ ...session, state: "idle" }, []);
  const starter = createOfferStarter(h.store);
  disposers.push(
    registerAuthoringOffers(({ offer, feature }) => starter.start(offer, feature)),
    starter.dispose,
  );
  h.mount(<EmptyOffer offer={offer.id} checklist={h.store.get(checklistAtom)} />);
  fireEvent.click(screen.getByRole("button", { name: offer.label }));
  expect(window.confirm).not.toHaveBeenCalled();
  expect(h.send).toHaveBeenCalledWith(expect.objectContaining({ command: "end" }));
});
