import { useAtomValue, useSetAtom } from "jotai";
import { FileText, X } from "lucide-react";
import type { JSX } from "react";

import { isActiveSession, panelSessionAtom } from "./session-home";
import { contextDismissedAtom, contextDocumentAtom } from "./viewed-document";
import { Untrusted, visibleControls } from "./visible-controls";

/**
 * What the chat knows the user is reading, as the first row of the composer's attachments: "About
 * section_05", or "Now viewing section_05" while a chat runs, where the next message carries the
 * page. It shares the attachment rows' anatomy (a glyph, the label, the remove at the end). It
 * follows the route, is not a live region (it changes on every navigation), and goes away with
 * its dismiss button until the user moves to another document. Its name is "Viewing <name>",
 * which the visible words do not repeat.
 */
export function ContextLine(): JSX.Element | null {
  const viewed = useAtomValue(contextDocumentAtom);
  const running = isActiveSession(useAtomValue(panelSessionAtom));
  const dismiss = useSetAtom(contextDismissedAtom);
  if (viewed.kind === null) return null;
  return (
    <div
      role="group"
      aria-label={`Viewing ${visibleControls(viewed.name)}`}
      className="chat-context-chip"
    >
      <span aria-hidden="true" className="chat-chip-glyph">
        <FileText />
      </span>
      <span className="chat-chip-label" title={visibleControls(viewed.name)}>
        {running ? "Now viewing" : "About"}{" "}
        <span className="chat-context-name">
          <Untrusted text={viewed.name} />
        </span>
      </span>
      <button
        type="button"
        className="chat-context-dismiss"
        aria-label="Dismiss context"
        onClick={() => dismiss({ path: viewed.path })}
      >
        <X size={14} aria-hidden="true" />
      </button>
    </div>
  );
}
