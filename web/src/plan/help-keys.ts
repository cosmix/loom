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
      ["Ctrl + S / ⌘ S", "Save"],
      ["Esc", "Leave the editor (closes the search panel first)"],
    ],
  },
];
