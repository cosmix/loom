import {
  BookOpenIcon,
  CompassIcon,
  FlaskConicalIcon,
  KeyboardIcon,
  LayersIcon,
  MessagesSquareIcon,
  ScrollTextIcon,
  type LucideIcon,
} from "lucide-react";
import type { ComponentType } from "react";

import { CHAT_SCENES, CHAT_TIPS } from "./chat-scene";
import { FEATURES_SCENES, FEATURES_TIPS } from "./features-scene";
import { GettingAroundScene } from "./getting-around-scene";
import { PlannerLegend } from "./legend";
import type { PlannerHelpScene } from "./pinned-scene";
import { PRESSURE_SCENES, PRESSURE_TIPS } from "./pressure-scene";
import { READING_SCENES, READING_TIPS } from "./reading-scene";
import { PlannerShortcuts } from "./shortcuts";

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
            target: ['[aria-label="Loom planner home"]', ".plan-identity"],
            text: "The Loom logo returns to the planner home, and the feature switcher shows the current feature and switches to another.",
          },
          {
            // Below 1024px the header shows only the "Step n of 6" button, not the rail.
            target: ['.plan-header ol[aria-label="Steps"]', ".plan-header .plan-rail-summary"],
            text: "The step rail counts the work in each step and links to it, and marks the current step.",
          },
          {
            // The wide card, or the compact button row below 1024px.
            target: [".plan-next-step", ".plan-next-action-compact"],
            text: "The next-step card offers the next action for the current planning work.",
          },
          {
            target: '[aria-label$="Waiting for you"]',
            text: "The activity pill counts what is in progress, such as a chat waiting for you, and opens the list.",
          },
          {
            target: "nav.view-switch",
            text: "Switch between the project home and the product, engineering and file views.",
          },
          {
            target: ['[aria-label="Settings"]', '[aria-label="Help"]'],
            text: "Settings changes planner preferences and appearance, and Help opens this guide with the legend and keyboard shortcuts.",
          },
        ],
      },
    ],
    tips: [
      "The step rail belongs to a feature and is hidden on the project home.",
      'The activity pill lists a running plan, a pressure test, a Codex review or a summary rebuild, and a chat that is starting, drafting a document, needs permission or is waiting for you; a chat that is working does not appear, and the pill reads "Idle" when nothing applies.',
    ],
  },
  {
    id: "reading",
    label: "Reading summaries",
    description: "Open sentences, follow marks and reach the source text.",
    Icon: ScrollTextIcon,
    hue: 2,
    intro:
      "Summaries are written in layers: open a sentence to see the sentences beneath it, and open those to reach the source passages.",
    scenes: READING_SCENES,
    tips: READING_TIPS,
  },
  {
    id: "features",
    label: "Features",
    description: "Start a feature, shape its vision and keep standalone plans.",
    Icon: LayersIcon,
    hue: 3,
    intro: "A feature groups the documents and plans for one piece of work in your project.",
    scenes: FEATURES_SCENES,
    tips: FEATURES_TIPS,
  },
  {
    id: "chat",
    label: "Chat and sessions",
    description: "Talk to a session, attach passages and answer permission requests.",
    Icon: MessagesSquareIcon,
    hue: 4,
    intro:
      "Sessions work on your documents while you chat; their tool use and changes appear inline.",
    scenes: CHAT_SCENES,
    tips: CHAT_TIPS,
  },
  {
    id: "pressure",
    label: "Pressure tests and runs",
    description: "Test a plan under pressure and follow a run step by step.",
    Icon: FlaskConicalIcon,
    hue: 5,
    intro:
      "A pressure test challenges a plan before it runs, and a run reports each step as it happens.",
    scenes: PRESSURE_SCENES,
    tips: PRESSURE_TIPS,
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
