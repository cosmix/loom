import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";

import { snapshotAtom } from "@/plan/chat/model";
import { harness, installStartChatLifecycle, session } from "@/plan/chat/start-chat-test-kit";
import { kickoffSessionAtom } from "@/plan/new/kickoff-state";
import { useKickoffStart, type PickedDocument, type WorkKind } from "@/plan/new/kickoff-start";

installStartChatLifecycle();

const document: PickedDocument = {
  id: "doc-1",
  label: "My notes",
  attachment: {
    kind: "source",
    path: "kickoff/notes.md",
    anchor: "",
    lines: null,
    stage: null,
    summary_sentence: null,
    rests_on: [],
    quote: "A place to collect project ideas.",
    selected_hash: null,
  },
};

/** A button per start, so a test drives the hook the way the screen will. */
function Probe({
  kind,
  feature,
  documents = [],
  onFollowUp,
}: {
  kind: WorkKind;
  feature: string | null;
  documents?: PickedDocument[];
  onFollowUp?: (feature: string) => void;
}) {
  const { start, pending, error } = useKickoffStart(onFollowUp);
  return (
    <>
      <button
        type="button"
        onClick={() => void start(kind, "Tighten the dialog", documents, feature)}
      >
        go
      </button>
      <span role="status">{pending ? "pending" : "idle"}</span>
      {error !== null && <span role="alert">{error}</span>}
    </>
  );
}

function mount(props: Parameters<typeof Probe>[0], current: typeof session | null = null) {
  const h = harness(current);
  h.mount(<Probe {...props} />);
  return h;
}

const go = () => fireEvent.click(screen.getByRole("button", { name: "go" }));

it("sends the kickoff offer for a feature and the small one for a plan", () => {
  const feature = mount({ kind: "feature", feature: null, documents: [document] });
  go();
  expect(feature.send).toHaveBeenCalledWith({
    type: "chat",
    command: "start",
    client_id: expect.any(String),
    offer: "kickoff",
    text: "Tighten the dialog",
    attachments: [document.attachment],
  });
});

it("sends kickoff-small for a standalone plan", () => {
  const plan = mount({ kind: "plan", feature: null });
  go();
  expect(plan.send).toHaveBeenCalledWith(
    expect.objectContaining({ offer: "kickoff-small", text: "Tighten the dialog" }),
  );
});

it("sends plan-follow-ups with the feature for a follow-up", () => {
  const h = mount({ kind: "follow-up", feature: "atlas", documents: [document] });
  go();
  expect(h.send).toHaveBeenCalledExactlyOnceWith({
    type: "chat",
    command: "start",
    client_id: expect.any(String),
    offer: "plan-follow-ups",
    text: "Tighten the dialog",
    attachments: [document.attachment],
    feature: "atlas",
  });
});

it("starts nothing for a follow-up without a feature", async () => {
  const h = mount({ kind: "follow-up", feature: null });
  go();
  await act(async () => {});
  expect(h.send).not.toHaveBeenCalled();
});

it("tells the screen which feature a follow-up started for once the command resolves", async () => {
  const onFollowUp = vi.fn();
  const h = mount({ kind: "follow-up", feature: "atlas", onFollowUp });
  go();
  expect(onFollowUp).not.toHaveBeenCalled();

  h.result();

  await waitFor(() => expect(onFollowUp).toHaveBeenCalledExactlyOnceWith("atlas"));
  expect(screen.getByRole("status").textContent).toBe("idle");
});

it("does not call onFollowUp when the start is refused", async () => {
  const onFollowUp = vi.fn();
  const h = mount({ kind: "follow-up", feature: "atlas", onFollowUp });
  go();

  h.result(0, "Another browser holds authoring");

  await waitFor(() => expect(screen.getByRole("alert")).toBeTruthy());
  expect(onFollowUp).not.toHaveBeenCalled();
});

it("leaves the kickoff session unset and waits for none on a follow-up", async () => {
  const h = mount({ kind: "follow-up", feature: "atlas" });
  go();
  h.result();
  await act(async () => {});
  act(() => h.store.set(snapshotAtom, { session, turns: [], queue: [], previous: null }));

  expect(h.store.get(kickoffSessionAtom)).toBeNull();
  expect(screen.getByRole("status").textContent).toBe("idle");
});

it("refuses a start while another chat is open", () => {
  const h = mount({ kind: "follow-up", feature: "atlas" }, session);
  go();

  expect(h.send).not.toHaveBeenCalled();
  expect(screen.getByRole("alert")).toBeTruthy();
});
