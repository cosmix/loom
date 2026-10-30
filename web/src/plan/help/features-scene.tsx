import { Provider } from "jotai/react";
import { useState } from "react";

import { projectChecklistViewSchema } from "@/plan/api/schema";
import { fileTreeSchema } from "@/plan/api/schema-files";
import { kickoffViewSchema } from "@/plan/api/schema-kickoff";
import { treeAtom } from "@/plan/files/atoms";
import type { PlannerHelpScene } from "@/plan/help/pinned-scene";
import { createSampleStore, SAMPLE_STEPS } from "@/plan/help/sample";
import { SceneRouter } from "@/plan/help/scene-router";
import { FeatureCards } from "@/plan/home/feature-card";
import { InvitationCard } from "@/plan/home/home";
import { StandaloneShelf } from "@/plan/home/standalone-shelf";
import { TakingShape } from "@/plan/new/taking-shape";
import { projectAtom } from "@/plan/state/atoms";

const SAMPLE_PROJECT = projectChecklistViewSchema.parse({
  checklists: [
    {
      feature: "atlas",
      pitch: "Plan field trips, track who signed up and keep every itinerary in one place.",
      has_product: true,
      sections: [],
      plans: { listed: 0, total: 0, written: 0, running: 0, done: 0 },
      untested: 0,
      metaplans: [],
      plan_rows: [],
      current: { kind: "step", number: 3 },
      steps: SAMPLE_STEPS,
      primary: null,
      secondary: [],
      items: [{ step: 2, text: "Product spec needs detail", path: null, offer: null }],
    },
  ],
  standalone: [
    {
      slug: "fix-search",
      title: "Fix search keyboard focus",
      path: "doc/plans/PLAN-fix-search.md",
      state: "written",
    },
  ],
  moves: [],
});

const SAMPLE_TREE = fileTreeSchema.parse({
  product: [
    {
      feature: "atlas",
      misplaced: false,
      path: "doc/product/atlas/index.md",
      target: null,
      title: "Atlas",
      file_name: "index.md",
      kind: "vision",
      state: null,
      locked: false,
      gap: false,
      pressure_date: null,
      errors: 0,
      blurb: "Help people find their way around a growing project.",
      children: [],
    },
  ],
  plans: [],
  knowledge: [],
  other: [],
  archive: [],
});

const SAMPLE_KICKOFF = kickoffViewSchema.parse({
  session: "atlas-help-kickoff",
  target: null,
  shape: {
    size: "feature",
    name: "atlas",
    proposal: false,
    problem: "People lose track of decisions as their project grows.",
    decisions: ["Keep the project map beside the documents it describes."],
    questions: ["How should archived work appear in the map?"],
    sections: [
      { title: "Project navigation", state: "settled" },
      { title: "Search and discovery", state: "wording" },
      { title: "Archived work", state: "open" },
    ],
  },
});

export function FeaturesHomeScene() {
  const [store] = useState(() => {
    const sample = createSampleStore();
    sample.set(projectAtom, SAMPLE_PROJECT);
    sample.set(treeAtom, SAMPLE_TREE);
    return sample;
  });

  // HomeRoute fetches the tree on mount, so this scene lays out what it renders: the invitation
  // beside the cards as production does, then the shelf.
  return (
    <Provider store={store}>
      <SceneRouter initialEntries={["/"]}>
        <div data-loom-plan inert>
          <div className="home">
            <div className="home__head">
              <p className="home__eyebrow">doc/ in this repository</p>
              <h2 className="home__title">Features</h2>
            </div>
            <FeatureCards lead={<InvitationCard empty={false} />} />
            <StandaloneShelf defaultOpen />
          </div>
        </div>
      </SceneRouter>
    </Provider>
  );
}

export function FeaturesKickoffScene() {
  // Its own store: the Write button reads the server's read-only state from the atoms.
  const [store] = useState(createSampleStore);
  return (
    <Provider store={store}>
      <div data-loom-plan inert>
        <div className="mx-auto max-w-md">
          <TakingShape shape={SAMPLE_KICKOFF.shape} target={SAMPLE_KICKOFF.target} />
        </div>
      </div>
    </Provider>
  );
}

export const FEATURES_SCENES: readonly PlannerHelpScene[] = [
  {
    title: "The project home",
    Scene: FeaturesHomeScene,
    notes: [
      {
        target: "section.invitation",
        text: "The invitation starts a conversation about a new feature or a standalone plan.",
      },
      {
        target: 'a[href="/f/atlas"]',
        text: "The feature card opens that feature's page.",
      },
      {
        target: 'ol[aria-label="Progress"]',
        text: "The progress strip marks each step as done, active, optional, or having gaps.",
      },
      {
        target: 'a[href="/files/plans/PLAN-fix-search.md"]',
        text: "A standalone plan row opens the plan document.",
      },
      {
        target: "section[aria-labelledby] button",
        text: "The shelf buttons start a conversation for a new standalone plan or open a dialog to move a plan into a feature.",
      },
    ],
  },
  {
    title: "Kicking off a feature",
    Scene: FeaturesKickoffScene,
    notes: [
      {
        target: 'aside[aria-label="Taking shape"] > section:nth-of-type(1)',
        text: "Size shows whether the conversation is shaping a feature or a standalone plan.",
      },
      {
        target: 'aside[aria-label="Taking shape"] > section:nth-of-type(2)',
        text: "Name shows the proposed name for the feature.",
      },
      {
        target: 'aside[aria-label="Taking shape"] > section:nth-of-type(3)',
        text: "Problem summarizes the need that the feature addresses.",
      },
      {
        target: 'aside[aria-label="Taking shape"] > section:nth-of-type(4)',
        text: "Settled decisions lists the choices already agreed in the conversation.",
      },
      {
        target: 'aside[aria-label="Taking shape"] > section:nth-of-type(5)',
        text: "Open questions lists what still needs to be decided.",
      },
      {
        target: 'aside[aria-label="Taking shape"] > section:nth-of-type(6)',
        text: "Sections shows the proposed sections and whether each is settled, being worded, or open.",
      },
    ],
  },
];

export const FEATURES_TIPS: readonly string[] = [
  "Nothing is written under doc/ until you say so in the kickoff conversation.",
  "Moving a standalone plan into a feature starts a conversation to relocate the document and fix its references.",
];
