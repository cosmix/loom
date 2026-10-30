import type { JSX } from "react";

import { ShortcutGroups } from "@/components/help-shortcuts";
import type { HelpShortcutGroup } from "@/components/help-topics";

export const PLANNER_SHORTCUT_GROUPS: HelpShortcutGroup[] = [
  {
    title: "While reading",
    rows: [
      ["↓ / j", "Focus the next openable sentence or fold cap"],
      ["↑ / k", "Focus the previous openable sentence or fold cap"],
      ["Enter / → / Space", "Open the focused closed sentence"],
      ["Esc", "Fold the innermost run around the focus, or else the run opened last"],
      ["Shift + Esc", "Fold every open run"],
      ["←", "Fold the innermost run around the focus"],
      ["f", "Open file mode at the first source section"],
      ["d", "Open the summary page of the first document"],
      ["a", "Attach the focused sentence or passage to the next chat message while authoring"],
      ["g p", "Go to the product overview"],
      ["g e", "Go to the engineering overview"],
    ],
  },
  {
    title: "Anywhere",
    rows: [
      ["?", "Open or close Help outside text fields"],
      ["Esc", "Close the active dialog"],
      ["c", "Focus the chat composer"],
      ["y", "Allow the pinned permission request"],
      ["n", "Deny it, with a note"],
      ["/", "Focus search in file mode outside text fields"],
      ["Esc", "Clear search in file mode"],
      ["Ctrl + S / ⌘ + S", "Save in the editor"],
      ["Esc", "Leave the editor (closes the search panel first)"],
    ],
  },
];

export function PlannerShortcuts(): JSX.Element {
  return (
    // Keys are at least 26px wide with a 2px bottom edge, as the dialog's other key hints.
    <div className="font-sans text-[13px] text-foreground [&_dd]:text-[13px]! [&_kbd]:inline-flex [&_kbd]:min-w-[26px] [&_kbd]:justify-center [&_kbd]:border-b-2 [&_kbd]:text-[13px]! [&_kbd]:shadow-[0_2px_0_var(--border)]">
      <ShortcutGroups groups={PLANNER_SHORTCUT_GROUPS} />
    </div>
  );
}
