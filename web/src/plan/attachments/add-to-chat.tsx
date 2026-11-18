import { useAtomValue, useSetAtom, useStore } from "jotai/react";
import { MessageSquarePlus } from "lucide-react";
import type { ReactNode, RefObject } from "react";

import type { SectionView } from "@/plan/api/schema-files";
import { elsewhereSessionAtom } from "@/plan/chat/session-home";
import { revealChat } from "@/plan/chat/sheet-state";
import { canAuthorAtom } from "@/plan/state/atoms";

import { addAttachmentAtom } from "./atoms";
import { useFloatingPlacement } from "./floating-position";
import { selectionSecondaries } from "./secondary";
import type { AttachmentDraft } from "./types";
import { useSelectionAttachment } from "./use-selection-attachment";
import { renderedViewProvenance } from "./view-provenance";

import "./add-to-chat.css";

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
  const tooLong = encoder.encode(draft.text).length > MAX_ATTACHMENT_BYTES;
  const secondaries = selectionSecondaries().flatMap((render, index) => {
    const node = render(draft);
    return node ? [<span key={index}>{node}</span>] : [];
  });
  if (!canAuthor && secondaries.length === 0) return null;
  const primary = canAuthor && !tooLong;

  return (
    <FloatingControl rect={rect}>
      {(primary || secondaries.length > 0) && (
        <div className="add-to-chat__pill">
          {primary && <PrimaryAction draft={draft} onAdded={onAdded} />}
          {primary && secondaries.length > 0 && (
            <span className="add-to-chat__rule" aria-hidden="true" />
          )}
          {secondaries}
        </div>
      )}
      {canAuthor && tooLong && (
        <p role="note" className="add-to-chat__note">
          Selection is over 16 KiB; select less to add it to chat.
        </p>
      )}
    </FloatingControl>
  );
}

/** The pill button that adds the selection: the action the selection exists for. */
function PrimaryAction({ draft, onAdded }: { draft: AttachmentDraft; onAdded: () => void }) {
  const add = useSetAtom(addAttachmentAtom);
  const store = useStore();
  return (
    <button
      type="button"
      className="add-to-chat__primary"
      onClick={() => {
        add(draft);
        // The chip lands in a composer the user can see: the panel is always shown on a wide
        // screen, and the sheet opens on a narrow one, as New chat does (chat/start-chat.tsx).
        revealChat(store);
        onAdded();
      }}
    >
      <MessageSquarePlus aria-hidden="true" />
      Add to chat
    </button>
  );
}

/** The actions group next to the selection; absent while the selection is outside the viewport. */
function FloatingControl({ rect, children }: { rect: DOMRect; children: ReactNode }) {
  const { ref, placement } = useFloatingPlacement(rect);
  if (!placement) return null;
  return (
    <div
      ref={ref}
      role="group"
      aria-label="Selection actions"
      className="add-to-chat"
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
