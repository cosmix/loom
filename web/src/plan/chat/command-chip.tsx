import { SquareSlash } from "lucide-react";
import type { JSX } from "react";

import type { Item } from "@/plan/api/schema-chat";
import { Untrusted, visibleControls } from "./visible-controls";
import "./command-menu.css";

type CommandRef = NonNullable<Extract<Item, { type: "user" }>["command"]>;

/**
 * A sent slash command in the user's bubble, in place of its typed line: the command's own name
 * (an alias shows as the command it ran) and its arguments, as one chip row.
 */
export function CommandChip({ command }: { command: CommandRef }): JSX.Element {
  const line = visibleControls(`/${command.name}${command.args === "" ? "" : ` ${command.args}`}`);
  return (
    <div className="chat-chip" data-kind="command" title={line}>
      <span aria-hidden="true" className="chat-chip-glyph">
        <SquareSlash />
      </span>
      <span className="chat-chip-label">
        /<Untrusted text={command.name} />
      </span>{" "}
      {command.args !== "" && (
        <span className="chat-command-chip-args">
          <Untrusted text={command.args} />
        </span>
      )}
    </div>
  );
}
