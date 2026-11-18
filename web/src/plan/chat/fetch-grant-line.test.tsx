import { act, fireEvent, render, screen } from "@testing-library/react";
import { Provider } from "jotai/react";
import { expect, it } from "vitest";

import type { SessionView } from "@/plan/api/schema-chat";
import { FetchGrantLine } from "./fetch-grant-line";
import { harness, snapshot, useFakeClock } from "./permission-card-test-kit";

useFakeClock();

function view(fields: Partial<SessionView>): SessionView {
  return { ...snapshot.session!, state: "working", ...fields };
}

function show(session: SessionView, viewer = false) {
  const h = harness([], viewer);
  render(
    <Provider store={h.store}>
      <FetchGrantLine session={session} />
    </Provider>,
  );
  return h;
}

it("draws nothing without a grant or once the session is over", () => {
  show(view({ fetch_grant: false }));
  show(view({ fetch_grant: true, state: "ended" }));

  expect(screen.queryByText("Web fetches allowed for this session")).toBeNull();
});

it("the holder's Ask again sends the revoke", async () => {
  const h = show(view({ fetch_grant: true }));

  expect(screen.getByText("Web fetches allowed for this session")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Ask again" }));
  expect(h.send).toHaveBeenLastCalledWith(
    expect.objectContaining({ type: "chat", command: "ask_fetches_again" }),
  );
  await act(async () => h.reply());
});

it("an observer sees the grant but not Ask again", () => {
  show(view({ fetch_grant: true }), true);

  expect(screen.getByText("Web fetches allowed for this session")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Ask again" })).toBeNull();
});
