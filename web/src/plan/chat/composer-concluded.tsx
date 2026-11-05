import { useAtomValue, useStore } from "jotai";
import { useLayoutEffect, useRef } from "react";

import { focusComposerAtom } from "./composer-hooks";
import { draftAtom } from "./drafts";
import { NewChatButton } from "./start-chat";

/** A draft the visible view cannot show: the one line that says it is still there. */
export function DraftKept() {
  const draft = useAtomValue(draftAtom);
  if (!draft.text && draft.attachments.length === 0) return null;
  return <p className="chat-composer-note">Your draft is kept</p>;
}

/**
 * New chat beside the box on a concluded chat. When the next chat starts the button leaves and the
 * box stays, so focus on the button moves to the box. A layout cleanup runs before React detaches
 * the button, so focus can still be read here.
 */
export function NewChatBesideBox() {
  const store = useStore();
  const area = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const node = area.current;
    return () => {
      if (node?.contains(document.activeElement)) store.set(focusComposerAtom, true);
    };
  }, [store]);
  return (
    <div ref={area}>
      <NewChatButton variant="concluded" />
    </div>
  );
}

/** The composer while the session writes its closing notes: no input, since nothing takes a message. */
export function EndingChat() {
  return (
    <>
      <p className="chat-composer-ending">The composer comes back with your next chat.</p>
      <DraftKept />
    </>
  );
}

/**
 * Section 08, Session states: an ended or failed chat says so and offers a new chat.
 * Starting the next chat swaps this view for the composer, and the button that held focus leaves
 * with it; `onLeave` reports whether focus was inside so the text box can take it.
 */
export function ConcludedChat({ onLeave }: { onLeave: (hadFocus: boolean) => void }) {
  const area = useRef<HTMLDivElement>(null);
  // A layout cleanup runs before React detaches the button, so focus can still be read here.
  useLayoutEffect(() => {
    const node = area.current;
    return () => onLeave(!!node?.contains(document.activeElement));
  }, [onLeave]);
  return (
    <div ref={area} className="chat-composer-concluded">
      <p className="chat-composer-ended">This chat has ended.</p>
      <NewChatButton variant="concluded" />
    </div>
  );
}
