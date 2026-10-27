import type { HelpShortcutGroup } from "@/components/help-topics";

export const PLAN_HELP_KEYS: HelpShortcutGroup[] = [
  { title: "General", rows: [["?", "Open or close Help"]] },
  {
    title: "File mode",
    rows: [
      ["/", "Focus search"],
      ["Esc", "Clear search"],
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
    title: "Concept mode",
    rows: [
      ["↓ / j", "Next sentence, entering opened runs"],
      ["↑ / k", "Previous sentence"],
      ["Enter / →", "Open the focused sentence"],
      ["Escape / ←", "Fold the innermost run around the focus"],
      ["f", "Open file mode at the first source section"],
      ["d", "Open the summary page of the first document"],
      ["a", "Attach the focused sentence to the next chat message"],
      ["g p", "Go to the product overview"],
      ["g e", "Go to the engineering overview"],
    ],
  },
];
