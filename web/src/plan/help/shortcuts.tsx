import type { JSX } from "react";

import { ShortcutGroups } from "@/components/help-shortcuts";
import type { HelpShortcutGroup } from "@/components/help-topics";

export const PLANNER_SHORTCUT_GROUPS: HelpShortcutGroup[] = [
  {
    title: "While reading",
    rows: [
      ["↓ / j", "Focus the next sentence or passage, including opened runs"],
      ["↑ / k", "Focus the previous sentence or passage"],
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
    title: "Editor",
    rows: [
      ["Ctrl + S / ⌘ + S", "Save"],
      ["Esc", "Leave the editor (closes the search panel first)"],
    ],
  },
  {
    title: "Chat",
    rows: [
      ["c", "Focus the chat composer"],
      ["y", "Allow the pinned permission request"],
      ["n", "Deny it, with a note"],
    ],
  },
  {
    title: "Anywhere",
    rows: [
      ["?", "Open or close Help outside text fields"],
      ["Esc", "Close the active dialog"],
      ["/", "Focus search in file mode outside text fields"],
      ["Esc", "Clear search in file mode"],
    ],
  },
];

export function PlannerShortcuts(): JSX.Element {
  return (
    <div className="font-sans text-[13px] text-foreground [&_dd]:text-[13px]! [&_kbd]:border-b-2 [&_kbd]:text-[13px]! [&_kbd]:shadow-sm">
      <ShortcutGroups groups={PLANNER_SHORTCUT_GROUPS} />
    </div>
  );
}
