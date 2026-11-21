import type { HistoryKind } from "@/api/schema";

/// What a saved screen is: the scrollback a session kept (`history`) or the one
/// screen a fullscreen session left behind (`last_screen`). The daemon sends it
/// as `terminal_history_kind`.
export type { HistoryKind };

export interface HistoryLabels {
  /// The stage action that opens the saved screen.
  button: string;
  /// The accessible name of that action.
  aria: string;
  /// The accessible name of the replay host.
  screenAria: string;
  eyebrow: string;
  footer: string;
  /// The link on a blocked stage's attention card.
  link: string;
  missing: string;
  failed: string;
  /// The hover text of the dialog's action.
  title: string;
}

const LABELS: Record<HistoryKind, HistoryLabels> = {
  history: {
    button: "Terminal history",
    aria: "open terminal history",
    screenAria: "Saved terminal history",
    eyebrow: "Terminal history",
    footer: "read-only · the session ended · this is its saved screen",
    link: "View terminal history",
    missing: "This session left no saved terminal history.",
    failed: "The saved terminal history could not be loaded.",
    title: "The session ended; its saved screen opens read-only",
  },
  last_screen: {
    button: "Last screen",
    aria: "open last screen",
    screenAria: "Saved last screen",
    eyebrow: "Last screen",
    footer: "read-only · the session ended · this is its last screen",
    link: "View last screen",
    missing: "This session left no saved screen.",
    failed: "The saved screen could not be loaded.",
    title: "The session ended; its last screen opens read-only",
  },
};

/// The wording for a saved screen of the given kind.
export function historyLabels(kind: HistoryKind): HistoryLabels {
  return LABELS[kind];
}
