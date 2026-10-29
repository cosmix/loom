import { render } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { MemoryRouter } from "react-router";

import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import hello from "@/plan/api/fixtures/hello.json";
import { serverInfoSchema } from "@/plan/api/schema";
import { sessionViewSchema, type SessionView } from "@/plan/api/schema-chat";
import type { DraftTeamView } from "@/plan/api/schema-kickoff";
import { snapshotAtom } from "@/plan/chat/model";
import { DraftTeamPanel } from "@/plan/feature/draft-team-panel";
import { authoringAtom, connectionAtom, draftTeamAtom, serverInfoAtom } from "@/plan/state/atoms";

export const server = serverInfoSchema.parse(hello.server);
export const session = sessionViewSchema.parse({ ...snapshotFixture.session, id: "draft-session" });
export const paths = [
  "doc/product/feature/section_01.md",
  "doc/product/feature/section_02.md",
  "doc/product/feature/section_03.md",
] as const;

export function teamView(): DraftTeamView {
  return {
    session: "draft-session",
    documents: [...paths],
    teammates: [
      {
        agent_id: "agent-1",
        name: "Ada",
        documents: [paths[0], paths[1]],
        current: paths[1],
        state: "working",
        tokens: { input: 1000, cache_write: 200, cache_read: 300, output: 400 },
      },
      {
        agent_id: "agent-2",
        name: "Grace",
        documents: [paths[2]],
        current: null,
        state: "working",
        tokens: { input: 2000, cache_write: 100, cache_read: 500, output: 500 },
      },
    ],
  };
}

export function storeFor(
  team: DraftTeamView | null = teamView(),
  current: SessionView | null = session,
) {
  const store = createStore();
  store.set(draftTeamAtom, team);
  store.set(snapshotAtom, { session: current, turns: [], queue: [], previous: null });
  store.set(serverInfoAtom, { ...server, read_only: [] });
  store.set(authoringAtom, { holder: null });
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: null });
  return store;
}

export function renderPanel(store = storeFor(), feature = "feature") {
  return render(
    <Provider store={store}>
      <MemoryRouter initialEntries={[`/f/${feature}`]}>
        <DraftTeamPanel feature={feature} />
      </MemoryRouter>
    </Provider>,
  );
}
