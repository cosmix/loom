import {
  BoldIcon,
  CodeIcon,
  HeadingIcon,
  ItalicIcon,
  LinkIcon,
  ListChecksIcon,
  ListIcon,
  ListOrderedIcon,
  QuoteIcon,
  SquareCodeIcon,
  TableIcon,
  type LucideIcon,
} from "lucide-react";
import type { JSX } from "react";

import type { FormatCommand } from "./codemirror";

const FORMATS: { command: FormatCommand; label: string; shortcut?: string; Icon: LucideIcon }[] = [
  { command: "bold", label: "Bold", shortcut: "Ctrl/Cmd+B", Icon: BoldIcon },
  { command: "italic", label: "Italic", shortcut: "Ctrl/Cmd+I", Icon: ItalicIcon },
  { command: "code", label: "Code", Icon: CodeIcon },
  { command: "link", label: "Link", shortcut: "Ctrl/Cmd+K", Icon: LinkIcon },
  { command: "heading", label: "Heading", Icon: HeadingIcon },
  { command: "bulletList", label: "Bulleted list", Icon: ListIcon },
  { command: "orderedList", label: "Numbered list", Icon: ListOrderedIcon },
  { command: "taskList", label: "Task list", Icon: ListChecksIcon },
  { command: "quote", label: "Quote", Icon: QuoteIcon },
  { command: "codeBlock", label: "Code block", Icon: SquareCodeIcon },
  { command: "table", label: "Table", Icon: TableIcon },
];

/** Icon buttons for the Markdown commands; each is named and titled with its shortcut. */
export function FormatButtons(props: {
  disabled: boolean;
  onFormat(command: FormatCommand): void;
}): JSX.Element {
  return (
    <div role="group" aria-label="Formatting" className="flex flex-wrap items-center gap-0.5">
      {FORMATS.map(({ command, label, shortcut, Icon }) => (
        <button
          key={command}
          type="button"
          aria-label={label}
          title={shortcut ? `${label} (${shortcut})` : label}
          disabled={props.disabled}
          onClick={() => props.onFormat(command)}
          className="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors outline-none hover:bg-muted hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-40"
        >
          <Icon className="size-4" aria-hidden="true" />
        </button>
      ))}
    </div>
  );
}
