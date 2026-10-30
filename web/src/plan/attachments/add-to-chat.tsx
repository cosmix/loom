import { useAtomValue, useSetAtom } from "jotai/react";
import type { ReactNode, RefObject } from "react";

import { Button } from "@/components/ui/button";
import type { SectionView } from "@/plan/api/schema-files";
import { elsewhereSessionAtom } from "@/plan/chat/session-home";
import { canAuthorAtom } from "@/plan/state/atoms";

import { addAttachmentAtom } from "./atoms";
import { useFloatingPlacement } from "./floating-position";
import { selectionSecondaries } from "./secondary";
import type { AttachmentDraft } from "./types";
import { useSelectionAttachment } from "./use-selection-attachment";
import { renderedViewProvenance } from "./view-provenance";

/** One attachment's text limit (section 12, "One attachment's text"). */
export const MAX_ATTACHMENT_BYTES = 16 * 1024;

const encoder = new TextEncoder();

type AddToChatProps = { draft: AttachmentDraft; rect: DOMRect; onAdded: () => void };

export function AddToChat({ draft, rect, onAdded }: AddToChatProps) {
  const holdsAuthoring = useAtomValue(canAuthorAtom);
  const elsewhere = useAtomValue(elsewhereSessionAtom);
  // An open session on another screen has no composer here, so its draft would hold an unseen
  // chip. A concluded one is not open: the next chat starts here, so attaching still works.
  const canAuthor = holdsAuthoring && elsewhere === null;
  const add = useSetAtom(addAttachmentAtom);
  const tooLong = encoder.encode(draft.text).length > MAX_ATTACHMENT_BYTES;
  const secondaries = selectionSecondaries().map((render, index) => {
    const node = render(draft);
    return node ? <span key={index}>{node}</span> : null;
  });
  if (!canAuthor && secondaries.every((node) => node === null)) return null;

  return (
    <FloatingControl rect={rect}>
      {canAuthor &&
        (tooLong ? (
          <span role="note" className="px-2 text-destructive">
            Selection is over 16 KiB; select less to add it to chat.
          </span>
        ) : (
          <Button
            type="button"
            size="sm"
            onClick={() => {
              add(draft);
              onAdded();
            }}
          >
            Add to chat
          </Button>
        ))}
      {secondaries}
    </FloatingControl>
  );
}

/** The actions box next to the selection; absent while the selection is outside the viewport. */
function FloatingControl({ rect, children }: { rect: DOMRect; children: ReactNode }) {
  const { ref, placement } = useFloatingPlacement(rect);
  if (!placement) return null;
  return (
    <div
      ref={ref}
      role="group"
      aria-label="Selection actions"
      className="fixed z-50 flex items-center gap-1.5 rounded-lg border bg-popover p-1 text-[13px] font-medium text-popover-foreground shadow-md"
      style={placement}
      // Keep the selection alive while the control is pressed.
      onMouseDown={(event) => event.preventDefault()}
    >
      {children}
    </div>
  );
}

type SelectionAttachProps = {
  container: RefObject<HTMLElement | null>;
  path: string;
  sections: SectionView[];
  /** `DocView.hash` of the document shown; recorded on the draft. */
  hash?: string | null;
};

/** Floating "Add to chat" control for a selection inside the rendered Markdown view. */
export function SelectionAttach({ container, path, sections, hash = null }: SelectionAttachProps) {
  const base = renderedViewProvenance(path, sections);
  const { draft, rect, clear } = useSelectionAttachment(container, (sel) => {
    const found = base(sel);
    return found && found.kind === "source" ? { ...found, hash } : found;
  });
  if (!draft || !rect) return null;
  return <AddToChat draft={draft} rect={rect} onAdded={clear} />;
}
