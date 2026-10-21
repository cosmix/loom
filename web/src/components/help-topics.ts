import {
  ActivityIcon,
  BookOpenIcon,
  CircleHelpIcon,
  GitBranchIcon,
  KeyboardIcon,
  ListChecksIcon,
  SlidersHorizontalIcon,
  TerminalIcon,
  TriangleAlertIcon,
  ZapIcon,
  type LucideIcon,
} from "lucide-react";

/// Where a note's numbered pin sits against its target: above it (the
/// default), on a corner, or beside it.
export type PinPlace = "t" | "tl" | "tr" | "l" | "r";

/// One numbered note, pinned to the element `at` selects inside its illustration.
export interface HelpNote {
  at: string;
  text: string;
  place?: PinPlace;
}

export type HelpVisualId =
  | "lifecycle"
  | "header"
  | "graph"
  | "ledger"
  | "card"
  | "attention"
  | "connection"
  | "terminal"
  | "resources"
  | "scopes";

export interface HelpScene {
  title: string;
  visual: HelpVisualId;
  notes: HelpNote[];
}

export interface HelpShortcutGroup {
  title: string;
  rows: readonly (readonly [string, string])[];
}

export interface HelpTopic {
  id: string;
  label: string;
  icon: LucideIcon;
  intro: string;
  scenes: HelpScene[];
  tips?: string[];
  shortcuts?: HelpShortcutGroup[];
}

export const HELP_TOPICS: HelpTopic[] = [
  {
    id: "start",
    label: "Getting started",
    icon: BookOpenIcon,
    intro:
      "Loom splits a plan into stages, runs each in its own worktree, checks it, and merges it.",
    scenes: [
      {
        title: "A stage’s journey",
        visual: "lifecycle",
        notes: [
          {
            at: "[data-pin=waiting]",
            text: "Waits until every dependency is completed and merged.",
          },
          {
            at: "[data-pin=queued]",
            text: "Ready, waiting for a free execution slot.",
            place: "tl",
          },
          {
            at: "[data-pin=executing]",
            text: "An agent session works in its own worktree.",
            place: "tl",
          },
          {
            at: "[data-pin=completed]",
            text: "Done and verified, but maybe not merged yet.",
            place: "tl",
          },
          {
            at: "[data-pin=merged]",
            text: "Integrated. Downstream stages can start.",
            place: "tl",
          },
        ],
      },
      {
        title: "Your controls",
        visual: "header",
        notes: [
          { at: "[data-pin=views]", text: "Switch between the graph and the ledger." },
          { at: "[data-pin=theme]", text: "Light or dark." },
          { at: "[data-pin=settings]", text: "Configuration, themes and notifications." },
          { at: "[data-pin=help]", text: "This guide. Press ? anywhere." },
        ],
      },
    ],
    tips: [
      "Skipping a stage does not satisfy its dependents.",
      "Copying a command never runs it. Paste it into your own shell.",
    ],
  },
  {
    id: "graph",
    label: "Graph & navigation",
    icon: GitBranchIcon,
    intro: "Two views of the same live run: a dependency map and a compact table.",
    scenes: [
      {
        title: "The graph",
        visual: "graph",
        notes: [
          {
            at: ".key-chip",
            text: "Hover a state to light its stages; click to pin it.",
            place: "l",
          },
          {
            at: ".react-flow__node[data-id=api] .stage-card",
            text: "Click a card to trace what it needs and what waits on it.",
            place: "l",
          },
          {
            at: ".react-flow__node[data-id=api] .stage-open",
            text: "Open the stage here, by double-click, or with Enter.",
          },
          { at: ".react-flow__controls-zoomin", text: "Zoom in and out.", place: "l" },
          {
            at: ".react-flow__controls-fitview",
            text: "Bring the whole plan back into frame.",
            place: "l",
          },
        ],
      },
      {
        title: "The ledger",
        visual: "ledger",
        notes: [
          { at: "[data-pin=views]", text: "Switch views in the header.", place: "r" },
          { at: "thead th:nth-child(2)", text: "One row per stage, indented by dependency depth." },
          { at: "thead th:nth-child(3)", text: "What each stage waits for." },
          { at: "tbody tr:nth-child(2) a", text: "Click a name to open the stage.", place: "r" },
          { at: "thead th:last-child", text: "Whether the work is merged yet." },
        ],
      },
    ],
    tips: ["Press Esc, or click empty canvas, to clear a highlight."],
  },
  {
    id: "stages",
    label: "Stage details",
    icon: ListChecksIcon,
    intro: "Everything a card tells you, and where to click for more.",
    scenes: [
      {
        title: "Reading a card",
        visual: "card",
        notes: [
          { at: ".stage-card header > :first-child", text: "The stage’s state." },
          { at: ".stage-open", text: "Open full details: session, dependencies, verification." },
          { at: ".terminal-glyph", text: "Open the live terminal.", place: "r" },
          {
            at: ".stage-tray > :first-child",
            text: "Activity, merge status and elapsed time.",
            place: "l",
          },
          {
            at: ".stage-tray [role=img]",
            text: "Context used against the session’s ceiling.",
            place: "l",
          },
          {
            at: ".node-tokens",
            text: "Fresh tokens per minute, and the recent trace.",
            place: "l",
          },
        ],
      },
    ],
    tips: [
      "Purple card: a contract writer is writing failing tests before implementation.",
      "Needs handoff: the session filled its context. A fresh one resumes the stage.",
    ],
  },
  {
    id: "legend",
    label: "Legend",
    icon: CircleHelpIcon,
    intro: "Every stage state, activity word, and resource indicator at a glance.",
    scenes: [],
  },
  {
    id: "attention",
    label: "Attention & recovery",
    icon: TriangleAlertIcon,
    intro: "Know when to step in, and when Loom is already on it.",
    scenes: [
      {
        title: "Cards that need you",
        visual: "attention",
        notes: [
          { at: "[data-pin=problem]", text: "What went wrong, and on which stage." },
          { at: "[data-pin=guidance]", text: "What to do about it. Read this first.", place: "l" },
          {
            at: "[data-pin=copy]",
            text: "Copy the fix command, then run it in your shell.",
            place: "r",
          },
          {
            at: "[data-pin=auto]",
            text: "Handled by loom: recovery is already running. No retry needed.",
            place: "r",
          },
        ],
      },
    ],
    tips: [
      "Waiting for input means the agent asked a question. Answer it in its terminal.",
      "A queued stage with no session is not necessarily stuck. Check the alert band first.",
    ],
  },
  {
    id: "activity",
    label: "Activity & connection",
    icon: ActivityIcon,
    intro: "Tell a quiet agent apart from a disconnected dashboard.",
    scenes: [
      {
        title: "Reading the daemon line",
        visual: "connection",
        notes: [
          { at: "[data-pin=live]", text: "Connected, and the daemon is running.", place: "r" },
          { at: "[data-pin=stopped]", text: "Connected, but the daemon has stopped.", place: "r" },
          {
            at: "[data-pin=offline]",
            text: "Feed lost. You see the last snapshot while it reconnects.",
            place: "r",
          },
        ],
      },
    ],
    tips: [
      "Recent activity starts empty after a reload. It lists only what this page has seen.",
      "Stale means no heartbeat for five minutes; crashed means the process died.",
    ],
  },
  {
    id: "terminals",
    label: "Terminals",
    icon: TerminalIcon,
    intro: "Watch an agent work, and take the keyboard when it needs an answer.",
    scenes: [
      {
        title: "Try it",
        visual: "terminal",
        notes: [
          {
            at: ".terminal-key",
            text: "Take control to type into the session. Release to go back.",
            place: "l",
          },
          {
            at: ".help-terminal-screen",
            text: "Stripes mean your keys reach the real agent.",
            place: "tl",
          },
          {
            at: ".terminal-hint",
            text: "Esc and Tab go to the agent while you control it.",
            place: "r",
          },
        ],
      },
    ],
    tips: [
      "No terminal? Start the dashboard with --terminals and use a tmux session.",
      "Reloading the page always returns to view mode.",
    ],
  },
  {
    id: "tokens",
    label: "Tokens & quotas",
    icon: ZapIcon,
    intro: "Session capacity, token spend, and provider limits are three different things.",
    scenes: [
      {
        title: "Three meters",
        visual: "resources",
        notes: [
          {
            at: ".help-stage-instruments > :first-child",
            text: "Context: how full this session is.",
            place: "l",
          },
          {
            at: ".help-stage-instruments .node-tokens",
            text: "Fresh tokens per minute, five-minute average.",
            place: "r",
          },
          {
            at: ".help-token-panel",
            text: "The plan’s token rate over time, by model.",
            place: "tl",
          },
          {
            at: ".help-quota-example",
            text: "Provider usage windows and when they reset.",
            place: "tl",
          },
        ],
      },
    ],
    tips: ["A missing meter means no data yet, not zero usage."],
  },
  {
    id: "settings",
    label: "Settings & appearance",
    icon: SlidersHorizontalIcon,
    intro: "Tune the dashboard, and see which configuration value wins.",
    scenes: [
      {
        title: "Which value applies",
        visual: "scopes",
        notes: [
          { at: "[data-pin=project]", text: "A project override wins over everything." },
          { at: "[data-pin=user]", text: "Your own value, used across projects." },
          {
            at: "[data-pin=default]",
            text: "Loom’s built-in value, used when nothing else is set.",
          },
          { at: "[data-pin=browser]", text: "Theme and notifications stay in this browser." },
        ],
      },
    ],
    tips: [
      "Press / to filter settings. Esc clears the filter, then closes.",
      "Watch for each control’s saved or error mark before trusting a change.",
      "Notifications need localhost or HTTPS, and an open dashboard tab.",
    ],
  },
  {
    id: "shortcuts",
    label: "Keyboard & controls",
    icon: KeyboardIcon,
    intro: "Shortcuts depend on where focus is. A controlled terminal keeps every key.",
    scenes: [],
    shortcuts: [
      {
        title: "Anywhere",
        rows: [
          ["?", "Open or close Help"],
          ["Esc", "Close a dialog"],
          ["Tab / Shift + Tab", "Move between controls"],
          ["Enter / Space", "Press the focused button"],
        ],
      },
      {
        title: "On the graph",
        rows: [
          ["Click", "Trace a stage, or pin a state"],
          ["Double-click", "Open a stage"],
          ["Enter", "Open the focused stage"],
          ["Esc / Click empty canvas", "Clear the highlight"],
        ],
      },
      {
        title: "In Settings",
        rows: [
          ["/", "Focus the filter"],
          ["Esc", "Clear the filter, then close"],
        ],
      },
    ],
    tips: ["In a controlled terminal every key, Esc included, goes to the agent."],
  },
];
