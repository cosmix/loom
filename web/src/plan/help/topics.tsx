import { BookOpenIcon, CompassIcon, KeyboardIcon, type LucideIcon } from "lucide-react";
import type { ComponentType } from "react";

import { GettingAroundScene } from "./getting-around-scene";
import { PlannerLegend } from "./legend";
import type { SceneNote } from "./pinned-scene";
import { PlannerShortcuts } from "./shortcuts";

export interface PlannerHelpScene {
  title: string;
  Scene: ComponentType;
  notes: readonly SceneNote[];
  scale?: number;
}

export interface PlannerHelpTopic {
  id: string;
  label: string;
  description: string;
  Icon: LucideIcon;
  hue: number; // 1..8, var(--doc-N)
  intro: string;
  scenes: readonly PlannerHelpScene[];
  Body?: ComponentType;
  tips?: readonly string[];
}

export const PLANNER_HELP_TOPICS: readonly PlannerHelpTopic[] = [
  {
    id: "getting-around",
    label: "Getting around",
    description: "Find your project, planning steps, views and settings.",
    Icon: CompassIcon,
    hue: 1,
    intro: "The header shows your project, planning progress and navigation controls.",
    scenes: [
      {
        title: "The planner header",
        Scene: GettingAroundScene,
        scale: 0.74,
        notes: [
          {
            target: ['[aria-label="Loom planner home"]', '[aria-label="Loom planner home"] + div'],
            text: "The project name identifies this workspace, and the Loom logo returns to its home.",
          },
          {
            target: '[aria-label="Steps"]',
            text: "The step rail shows the current step and which steps are done, have gaps or are optional.",
          },
          {
            target: '.plan-header div:has(> [title="Start a metaplan"])',
            text: "The next-step card offers the next action for the current planning work.",
          },
          {
            target: "nav.view-switch",
            text: "Switch between the project home and the product, engineering and file views.",
          },
          {
            target: '[aria-label="Settings"]',
            text: "Open Settings to change planner preferences and appearance.",
          },
          {
            target: '[aria-label="Help"]',
            text: "Open Help to read this guide, the glyph legend and keyboard shortcuts.",
          },
        ],
      },
    ],
    tips: [
      "The step rail belongs to a feature and is hidden on the project home.",
      "The activity pill appears while work is in progress and is absent when idle.",
    ],
  },
  {
    id: "legend",
    label: "Legend",
    description: "Read the marks, document glyphs and activity indicators.",
    Icon: BookOpenIcon,
    hue: 6,
    intro: "The legend names the marks, document types, states and activities used in the planner.",
    scenes: [],
    Body: PlannerLegend,
  },
  {
    id: "shortcuts",
    label: "Shortcuts",
    description: "Use the keyboard to read, navigate and open dialogs.",
    Icon: KeyboardIcon,
    hue: 8,
    intro: "These keys move through reading passages, switch views and control dialogs.",
    scenes: [],
    Body: PlannerShortcuts,
  },
];
