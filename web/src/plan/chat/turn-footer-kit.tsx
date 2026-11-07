import { render } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { MemoryRouter } from "react-router";

import snapshot from "@/plan/api/fixtures/chat-snapshot.json";
import treeFixture from "@/plan/api/fixtures/files-tree.json";
import docFixture from "@/plan/api/fixtures/files-doc.json";
import checklist from "@/plan/api/fixtures/checklist.json";
import { projectChecklistViewSchema } from "@/plan/api/schema";
import { chatSnapshotSchema, type Turn } from "@/plan/api/schema-chat";
import { authoringAtom, projectAtom, youAtom } from "@/plan/state/atoms";
import { draftAtom } from "./drafts";
import { TurnFooter } from "./turn-footer";

export const path = "doc/product/demo/section_02.md";
const plans = [
  { slug: "draft", title: "Draft plan", state: "written" },
  { slug: "active", title: "Active plan", state: "running" },
  { slug: "built", title: "Built plan", state: "done" },
];
const fixture = chatSnapshotSchema.parse(snapshot).turns[0];
// Shared request log and per-test overrides for `respond`; `resetNet` clears them in place.
export const requests: string[] = [];
export const failures: Record<string, string> = {};
export const diffs: Record<string, { before: string | null; after: string | null }> = {};
export const sourcePaths = [
  path,
  "doc/plans/PLAN-draft.md",
  "doc/plans/IN_PROGRESS-PLAN-active.md",
  "doc/plans/DONE-PLAN-built.md",
];

export function resetNet() {
  requests.length = 0;
  for (const key of Object.keys(failures)) delete failures[key];
  for (const key of Object.keys(diffs)) delete diffs[key];
}

export function turn(overrides: Partial<Turn> = {}): Turn {
  return {
    ...fixture,
    id: "t2",
    open: false,
    duration_ms: 65000,
    tokens: { input: 1000, cache_write: 200, cache_read: 9000, output: 300 },
    changed_files: [{ path, before_hash: "before", by_tool: true }],
    summaries_rebuilding: [],
    ...overrides,
  };
}

export function mount(value = turn(), holder = true) {
  const store = createStore();
  const project = projectChecklistViewSchema.parse({
    checklists: [
      {
        ...checklist.checklists[0],
        items: [{ step: 4, text: "Draft plan", path: "doc/plans/PLAN-draft.md", offer: null }],
      },
    ],
    standalone: [
      {
        slug: "y",
        path: "doc/plans/PLAN-y.md",
        state: "written",
        title: "Standalone",
        last_change: null,
        has_summary: false,
      },
    ],
    moves: [],
  });
  store.set(projectAtom, project);
  const you = { public_id: "ada", name: "Ada" };
  store.set(youAtom, you);
  store.set(authoringAtom, {
    holder: { ...you, public_id: holder ? "ada" : "other", away_since: null, frees_at: null },
  });
  store.set(draftAtom, { text: "Keep until fill succeeds", attachments: [] });
  const view = render(
    <Provider store={store}>
      <MemoryRouter>
        <TurnFooter turn={value} sessionId="session one" />
      </MemoryRouter>
    </Provider>,
  );
  return { store, ...view };
}

export async function respond(input: RequestInfo | URL): Promise<Response> {
  const url = new URL(String(input), "http://localhost");
  requests.push(url.pathname + url.search);
  const target = url.searchParams.get("path") ?? "";
  if (failures[url.pathname])
    return Response.json({ error: "refused", message: failures[url.pathname] }, { status: 503 });
  if (url.pathname === "/api/implemented-by")
    return Response.json({ sections: { "product/demo/section_02": plans } });
  if (url.pathname === "/api/diff/turn")
    return Response.json({
      path: `doc/${target}`,
      ...(diffs[target] ?? { before: "old\nkeep\n", after: "new\nkeep\n" }),
    });
  if (url.pathname === "/api/tree") {
    return Response.json({
      ...treeFixture,
      plans: sourcePaths.slice(1).map((name, index) => ({
        ...treeFixture.plans[0],
        path: name,
        file_name: name.split("/").at(-1),
        state: plans[index].state,
      })),
    });
  }
  if (url.pathname === "/api/doc")
    return Response.json({
      ...docFixture,
      path: `doc/${target}`,
      text: `Source of ${target}`,
      hash: `hash:${target}`,
    });
  return Response.json({ error: "not_found" }, { status: 404 });
}
