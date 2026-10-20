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
} from "lucide-react";

export const HELP_TOPICS = [
  {
    id: "start",
    label: "Getting started",
    icon: BookOpenIcon,
    intro: "Follow a plan from the first session to the final merge.",
    sections: [
      {
        title: "Your live view of a run",
        body: "Loom breaks a plan into stages, runs agent sessions in separate worktrees, checks their work, and merges the results. This dashboard shows the dependency graph, live session activity, verification outcomes, and the work that needs your attention.",
      },
      {
        title: "A useful first pass",
        steps: [
          "Check the header for the plan name, daemon status, and overall progress.",
          "Use the graph to see which stages can run together and what is holding up downstream work.",
          "Open a stage for its session, dependencies, verification results, and operator guidance.",
          "Read Needs attention for decisions you must make; Handled by loom shows recovery already in progress.",
        ],
      },
      {
        title: "When is a stage really finished?",
        body: "Completed means the work is done and verified. It may still be waiting to merge. Dependencies need completed, merged work before they can proceed; skipping a stage does not satisfy its dependents. Check both the stage state and its merge status.",
      },
      {
        title: "What you can do here",
        body: "Inspect stages, follow live terminals when enabled, change settings, and copy the commands shown in attention cards. Run copied commands in your own shell in the project. Copying a command does not execute it.",
      },
    ],
  },
  {
    id: "graph",
    label: "Graph & navigation",
    icon: GitBranchIcon,
    intro: "Read the dependencies, trace a thread, and open the stage behind it.",
    sections: [
      {
        title: "Trace the plan",
        body: "Each card is a stage; connecting threads show dependencies. Click a stage once to highlight its upstream and downstream thread. Click it again, click empty space, or press Escape while the graph has focus to clear the selection.",
      },
      {
        title: "Open a stage",
        body: "Double-click a card, press Enter on a focused card, or use its corner open button. The stage dialog can open from either view. Its full-page link gives you more room, and the browser Back button closes a stage opened through navigation.",
      },
      {
        title: "Find a state",
        body: "Hover a state chip above the graph to highlight matching stages. Click the chip to pin that highlight; click again to clear it. The state key only shows states present in the current plan.",
      },
      {
        title: "Move around",
        body: "Drag the canvas to pan. Use the zoom controls to move in or out, and Fit view to bring the plan back into frame. Cards stay in their dependency layout; dragging them does not rearrange the plan.",
      },
      {
        title: "Switch to the ledger",
        body: "Use the graph and ledger buttons in the header to switch between the dependency map and the compact stage table. Both show the same live run. The Loom logo returns you to the overview.",
      },
    ],
  },
  {
    id: "stages",
    label: "Stage details",
    icon: ListChecksIcon,
    intro: "Understand what a stage is doing and what it is waiting for.",
    sections: [
      {
        title: "Read beyond the badge",
        body: "Open a stage to inspect its activity, model and session, dependencies, context use, verification, merge information, and available guidance. Fields appear when they apply and data is available; hover field labels for explanations.",
      },
      {
        title: "Dependencies and readiness",
        body: "Waiting stages still have dependencies to satisfy. Queued stages are ready but need a free execution slot. An executing stage has a working session. A stage can be ready without starting immediately when concurrency limits or scheduling conditions prevent a launch.",
      },
      {
        title: "Contract-writing phase",
        body: "A purple card marks a contract writer creating failing tests before implementation. It is still an executing standard stage. The card returns to the execution tone when the implementation session takes over.",
      },
      {
        title: "Handoffs",
        body: "Needs handoff means the session reached its context ceiling. A fresh session can resume the stage with its handoff information. This is different from a process crash or a failed acceptance check.",
      },
      {
        title: "Verification and merge",
        body: "Inspect the failure evidence and operator guidance before retrying or reviewing a stage. Completed work can remain unmerged; merge conflicts and merge errors have their own states and recovery sessions. The header’s merge counts show the remaining integration work.",
      },
    ],
  },
  {
    id: "legend",
    label: "Legend",
    icon: CircleHelpIcon,
    intro: "Every stage state, activity word, and resource indicator at a glance.",
    sections: [],
  },
  {
    id: "attention",
    label: "Attention & recovery",
    icon: TriangleAlertIcon,
    intro: "Know when to step in and when Loom is already handling it.",
    sections: [
      {
        title: "Needs attention",
        body: "These cards identify work waiting on you. Read the evidence, the complete note, and the suggested action. Open the linked stage for context, then copy the supplied command if you need to act from your shell. The header’s attention count includes operator items.",
      },
      {
        title: "Handled by loom",
        body: "These entries describe automatic work such as merge resolution or adjudication. Read the note to see what recovery is doing. An automatic entry is not a request for a manual retry.",
      },
      {
        title: "Waiting for input",
        body: "The agent needs an answer in its session. Open its terminal when available, take control to respond, then release control when finished. If browser terminals are unavailable, use the session access command shown by the dashboard.",
      },
      {
        title: "Review and retry",
        body: "A human-review request asks for a decision; the card shows the available review choices. A blocked or failed stage may need its underlying problem fixed before retrying. Follow the stage-specific command and note: forcing completion and approving a review have different consequences.",
      },
      {
        title: "Scheduling alerts",
        body: "The alert band explains run-wide scheduling conditions. A queued stage with no session is not necessarily stuck: inspect these alerts and the daemon state before intervening.",
      },
    ],
  },
  {
    id: "activity",
    label: "Activity & connection",
    icon: ActivityIcon,
    intro: "Separate a quiet agent from a disconnected dashboard.",
    sections: [
      {
        title: "The live feed",
        body: "The header reports both feed health and daemon state. If the feed drops, the dashboard keeps the last snapshot, marks the daemon unknown, and shows how long it has been without data. It reconnects automatically. Frozen cards are not proof that sessions have stopped.",
      },
      {
        title: "A stopped daemon",
        body: "A live feed can report a stopped daemon. That is different from an offline browser connection. Check the daemon state and scheduling alerts when progress pauses.",
      },
      {
        title: "Recent activity",
        body: "The Activity panel lists stage transitions this page has observed, newest first. The initial snapshot is its baseline, so an empty panel after opening or reloading the page does not mean the run has done no work.",
      },
      {
        title: "Session activity",
        body: "Working means recent tool use; idle means an alive session with no current activity. Stale means no heartbeat for five minutes. Orphaned means execution has no session record; crashed means the process died. The Legend topic keeps the full activity key alongside stage states.",
      },
    ],
  },
  {
    id: "terminals",
    label: "Terminals",
    icon: TerminalIcon,
    intro: "Watch a session, then deliberately take control when you need to answer.",
    sections: [
      {
        title: "Open the session",
        body: "Use the terminal glyph on a stage or its Terminal button. You can inspect it in the stage dialog or open the dedicated terminal page for more space. Availability depends on how the dashboard was started and the stage’s session.",
      },
      {
        title: "View and control",
        body: "Terminals open in view mode. Take control to send keyboard input to the agent’s session. Release control returns to viewing; reloading also returns to view mode. Keystrokes in control mode affect the real session.",
      },
      {
        title: "Escape stays with the agent",
        body: "While controlling a terminal, Escape and Tab are terminal input. Escape does not release control or close the stage dialog. Use the release control switch first, then close the dialog or return to stage details.",
      },
      {
        title: "If a terminal is unavailable",
        body: "Read the terminal’s notice for the exact reason and next step. Live browser terminals require the dashboard’s --terminals option and a supported live tmux session. Remote access can require the authenticated startup URL. Use the retry control after resolving a connection problem.",
      },
    ],
  },
  {
    id: "tokens",
    label: "Tokens & quotas",
    icon: ZapIcon,
    intro: "Distinguish session capacity, token consumption, and provider limits.",
    sections: [
      {
        title: "Context is resident capacity",
        body: "The stage’s context meter compares resident session tokens with its context ceiling. It describes how full this session is, rather than all tokens spent on the stage. A handoff lets a fresh session continue when the ceiling is reached.",
      },
      {
        title: "Fresh token flow",
        body: "Fresh tokens combine uncached input, cache writes, and output. The flow chart shows the rate over time; fresh tokens per minute uses a five-minute average. Cached reads are accounted for separately through cache-hit information.",
      },
      {
        title: "Totals and models",
        body: "The header shows the plan token total and cache hit when available. The token-flow panel provides the rate chart and model breakdown. Stage cards can show their own token traces. A missing meter means no corresponding data is available, not necessarily zero usage.",
      },
      {
        title: "Provider quotas",
        body: "Footer gauges show the provider usage windows and reset information that Loom can retrieve. The five-hour time-left estimate uses the last thirty minutes’ average consumption; changing activity changes that estimate. Provider quota and a session’s context ceiling measure different limits.",
      },
    ],
  },
  {
    id: "settings",
    label: "Settings & appearance",
    icon: SlidersHorizontalIcon,
    intro: "Tune the dashboard and inspect the configuration behind the run.",
    sections: [
      {
        title: "Find a setting",
        body: "Open Settings in the header. Filter by key, value, or help text; press / outside a field to focus the filter. With a non-empty filter focused, Escape clears it first. Press Escape again to close Settings.",
      },
      {
        title: "Scope and effective values",
        body: "The configuration shows user and project scopes alongside the effective value. Project overrides take precedence over user values, with defaults beneath both. Removing an override restores the fallback; it does not necessarily turn a setting off.",
      },
      {
        title: "Save feedback",
        body: "Configuration edits are written individually. Watch each control’s pending, saved, or error feedback. Remote writes require authorization from the dashboard startup URL. A failed write needs attention before you assume the new value is active.",
      },
      {
        title: "Appearance",
        body: "Choose Ledger, Aubergine, Pacific, or Graphite in the dashboard settings, or use the header’s theme toggle. Appearance preferences are saved in this browser, separately from project and user configuration.",
      },
      {
        title: "Notifications",
        body: "Enable browser notifications in Settings and allow the browser permission prompt. Notifications cover new attention items, handoffs, and a finished run. They require a secure context such as localhost or HTTPS; plain HTTP on a remote host cannot enable them. Keep the dashboard open to receive updates.",
      },
    ],
  },
  {
    id: "shortcuts",
    label: "Keyboard & controls",
    icon: KeyboardIcon,
    intro: "The shortcuts depend on where focus is. Terminal control keeps its own keys.",
    sections: [
      {
        title: "Across the dashboard",
        rows: [
          ["?", "Open or close Help, outside text inputs."],
          [
            "Esc",
            "Close a dialog. Terminal control and a populated settings filter have special behavior.",
          ],
          ["Tab / Shift + Tab", "Move between controls; focus stays inside an open dialog."],
          ["Enter / Space", "Activate a focused button."],
        ],
      },
      {
        title: "On the graph",
        rows: [
          ["Click", "Trace a stage’s thread, or pin a state chip."],
          ["Double-click", "Open a stage card."],
          ["Enter", "Open the focused stage card."],
          ["Esc / empty canvas", "Clear the graph’s pinned highlight."],
        ],
      },
      {
        title: "In Settings",
        rows: [
          ["/", "Focus the filter when outside a field."],
          ["Esc in the filter", "Clear a non-empty filter before closing Settings."],
        ],
      },
      {
        title: "In a controlled terminal",
        body: "Keyboard input, including Escape and Tab, goes to the session. Use release control to return to view mode and normal dashboard keyboard navigation.",
      },
    ],
  },
] as const;

export type HelpTopic = (typeof HELP_TOPICS)[number];
export interface HelpSection {
  title: string;
  body?: string;
  steps?: readonly string[];
  rows?: readonly (readonly [string, string])[];
}
