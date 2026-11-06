import { useId } from "react";

/**
 * The panel before any message: no session exists yet, and the first Send starts it. It replaces
 * the transcript, so a fresh page and a cleared chat read the same.
 */
export function EmptyChat() {
  const heading = useId();
  return (
    <section className="chat-empty" aria-labelledby={heading}>
      <h3 id={heading} className="chat-empty-title">
        Let's spec!
      </h3>
      <p className="chat-empty-lede">
        I'll read the documents beside you and write down what we settle. Your first message starts
        the session.
      </p>
    </section>
  );
}
