import { createStore } from "jotai";
import type { Atom } from "jotai";
import { Provider } from "jotai/react";
import { loadable } from "jotai/utils";
import { INTERNAL_getBuildingBlocksRev3 } from "jotai/vanilla/internals";
import { useState } from "react";
import type { JSX } from "react";

import { pageHref, summaryKey } from "@/plan/api/page-key";
import type { ConceptPage, Mark, Sentence } from "@/plan/api/schema-concept";
import type { DocView } from "@/plan/api/schema-files";
import { conceptPageAtom } from "@/plan/concept/page-state";
import { docViewAtom } from "@/plan/concept/passages";
import { ReadingColumn } from "@/plan/concept/reading-column";
import type { OpenRun } from "@/plan/concept/types";
import { openRunsAtom } from "@/plan/concept/view-state";
import type { PlannerHelpScene } from "@/plan/help/pinned-scene";
import { SceneRouter } from "@/plan/help/scene-router";

const SOURCE = { path: "doc/product/map-view.md", anchor: "saved-places" };
const CURRENT_MARK: Mark = {
  doc: {
    id: summaryKey("help", "product", "map-view"),
    title: "Map view",
    path: SOURCE.path,
    page: summaryKey("help", "product", "map-view"),
  },
  colour: 1,
  out_of_date: null,
};
const OUTDATED_MARK: Mark = {
  doc: {
    id: summaryKey("help", "product", "offline-access"),
    title: "Offline access",
    path: "doc/product/offline-access.md",
    page: summaryKey("help", "product", "offline-access"),
  },
  colour: 2,
  out_of_date: { reason: "source_changed", detail: null },
};
const SUMMARY_SENTENCE: Sentence = {
  id: "help-summary",
  text: "People can **save places** and keep their maps available offline.",
  marks: [CURRENT_MARK, OUTDATED_MARK],
  citations: [
    { kind: "source", ...SOURCE },
    { kind: "source", path: OUTDATED_MARK.doc.path, anchor: "offline-maps" },
  ],
  rests_on: [SOURCE],
  plans: [],
};
const FOLD_SENTENCE: Sentence = {
  id: "help-saved-places",
  text: "Saved places stay on the map for the next visit.",
  marks: [CURRENT_MARK],
  citations: [{ kind: "source", ...SOURCE }],
  rests_on: [SOURCE],
  plans: [],
};
const SOURCE_TEXT =
  "## Saved places\n\nSave a place from the map to find it again on your next visit.";
const SOURCE_DOC: DocView = {
  feature: "atlas",
  misplaced: false,
  path: SOURCE.path,
  title: "Map view",
  text: SOURCE_TEXT,
  hash: "help-map-view",
  kind: "section",
  editable: false,
  lock: null,
  sections: [
    { level: 2, title: "Saved places", anchor: SOURCE.anchor, start_line: 1, end_line: 3 },
  ],
  plan: null,
  too_large: false,
  truncated: false,
  size: SOURCE_TEXT.length,
};

function samplePage(key: string, sentence: Sentence): ConceptPage {
  return {
    feature: "help",
    key,
    title: "Map view",
    path: SOURCE.path,
    file_hash: "help-map-view",
    colour: 1,
    state: { state: "current", out_of_date: null },
    plan_state: null,
    last_pressure: null,
    key_chips: [],
    blocks: [{ kind: "paragraph", sentences: [sentence] }],
    unlisted: [],
    implements: null,
    implemented_by: [],
    empty: null,
    building: null,
    estimate: null,
  };
}

const SENTENCE_PAGE = samplePage(
  summaryKey("help", "product", "reading-sentence"),
  SUMMARY_SENTENCE,
);
const FOLD_PAGE = samplePage(summaryKey("help", "product", "reading-fold"), FOLD_SENTENCE);
const OPEN_RUN: OpenRun = {
  level: 1,
  sentenceId: FOLD_SENTENCE.id,
  restsOn: [SOURCE],
  parent: null,
};

// docViewAtom is a read-only derived atom that fetches when read, so no public setter can
// preload it; seed this store's cache directly. The seed reaches the reader because jotai's
// `loadable()` memoizes per base atom (WeakMap), so the instance built here is the one
// concept/passage-doc.ts builds for the same atom. This cache shape is tied to the pinned Jotai
// 2.20.3 internal API: revisit on any Jotai major upgrade, or
// when docViewAtom exposes a writable seed or Jotai a public one. openRunsAtom is seeded the
// same way, because its setter would write sessionStorage on every mount of the dialog.
function seedSample<Value>(
  store: ReturnType<typeof createStore>,
  target: Atom<Value>,
  value: Value,
): void {
  const [states] = INTERNAL_getBuildingBlocksRev3(store);
  states.set(target, { d: new Map(), p: new Set(), n: 1, v: value });
}

function sampleStore(page: ConceptPage, open: OpenRun[]): ReturnType<typeof createStore> {
  const store = createStore();
  store.set(conceptPageAtom(page.key), { page, previous: null, error: null });
  seedSample(store, openRunsAtom(page.key), open);
  seedSample(store, loadable(docViewAtom(SOURCE.path)), { state: "hasData", data: SOURCE_DOC });
  return store;
}

function ReadingSample({
  store,
  page,
}: {
  store: ReturnType<typeof createStore>;
  page: ConceptPage;
}): JSX.Element {
  return (
    <Provider store={store}>
      <SceneRouter initialEntries={[pageHref(page.key)]}>
        <div data-loom-plan inert style={{ maxWidth: 640 }}>
          <ReadingColumn pageKey={page.key} page={page} />
        </div>
      </SceneRouter>
    </Provider>
  );
}

export function ReadingSentenceScene(): JSX.Element {
  const [store] = useState(() => sampleStore(SENTENCE_PAGE, []));
  return <ReadingSample store={store} page={SENTENCE_PAGE} />;
}

export function ReadingFoldScene(): JSX.Element {
  const [store] = useState(() => sampleStore(FOLD_PAGE, [OPEN_RUN]));
  return <ReadingSample store={store} page={FOLD_PAGE} />;
}

export const READING_SCENES: readonly PlannerHelpScene[] = [
  {
    title: "A summary sentence and its marks",
    Scene: ReadingSentenceScene,
    notes: [
      {
        target: '.concept-sentence[data-sentence-id="help-summary"]',
        text: "The summary sentence opens its supporting detail in place when you click it.",
      },
      {
        target: ".concept-mark:not([data-dashed])",
        text: "The solid colour mark opens the summary page of a document this sentence draws on.",
      },
      {
        target: '.concept-mark[data-dashed="true"]',
        text: "The dashed mark opens the summary page of a document whose summary is out of date.",
      },
    ],
  },
  {
    title: "Folding and sources",
    Scene: ReadingFoldScene,
    notes: [
      {
        target: '.concept-run-cap[data-side="start"]',
        text: "The fold cap at the start of the open passage folds it back into its summary sentence.",
      },
      {
        target: ".concept-passage-chip",
        text: "The source chip names the file and section and links to those lines in the full file.",
      },
      {
        target: ".concept-passage .md",
        text: "The source excerpt shows the text of the section that supports the summary sentence.",
      },
    ],
  },
];

// Verified against marks.tsx, sentence.tsx and column-focus.ts in plan/concept.
export const READING_TIPS: readonly string[] = [
  "A dashed mark can mean that a source changed, a rebuild is running, approval is pending, or a build cannot finish.",
  "Selecting text in a summary sentence leaves its detail closed so you can use the selection in chat.",
  "Opening detail preserves the scroll position unless the opened point would be outside the viewport.",
];
