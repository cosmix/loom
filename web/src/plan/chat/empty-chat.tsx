import { useId } from "react";

/** The greeting by the local clock at render: morning before 12:00, afternoon before 18:00. */
function greeting(now: Date): string {
  const hour = now.getHours();
  if (hour < 12) return "Good morning";
  if (hour < 18) return "Good afternoon";
  return "Good evening";
}

/**
 * The panel before any message: no session exists yet, and the first Send starts it. It replaces
 * the transcript, so a fresh page and a cleared chat read the same.
 */
export function EmptyChat() {
  const heading = useId();
  return (
    <section className="chat-empty" aria-labelledby={heading}>
      <h3 id={heading} className="chat-empty-title">
        {greeting(new Date())}. What are we deciding?
      </h3>
      <p className="chat-empty-lede">
        I'll read the documents beside you and write down what we settle. Your first message starts
        the session.
      </p>
    </section>
  );
}
