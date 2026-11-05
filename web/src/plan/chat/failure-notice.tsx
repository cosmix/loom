import type { ReactNode } from "react";

import { useNow } from "@/components/hooks/use-now";
import { formatElapsed } from "@/lib/format";
import type { SessionView } from "@/plan/api/schema-chat";
import { Untrusted } from "./visible-controls";

function age(since: string, now: number): string {
  const seconds = Math.floor((now - Date.parse(since)) / 1000);
  return formatElapsed(Number.isFinite(seconds) ? Math.max(0, seconds) : 0);
}

/** A failure notice: a toned glyph and a title over plain text, with no box. */
function Notice({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="chat-notice" data-tone="error">
      <p className="chat-notice-title">
        <span aria-hidden="true" className="chat-notice-glyph">
          ⚠
        </span>
        {title}
      </p>
      {children}
    </div>
  );
}

/** What a failed session says: why it stopped, when it last did anything, what the documents keep. */
export function FailureNotice({ session }: { session: SessionView }) {
  const now = useNow();
  const title =
    session.failure?.kind === "spawn"
      ? "The session could not start"
      : "The session stopped unexpectedly";
  return (
    <Notice title={title}>
      {session.failure && (
        <p>
          <Untrusted text={session.failure.message} />
        </p>
      )}
      {session.failure?.kind !== "spawn" && session.last_activity && (
        <p>
          Last activity{" "}
          <time dateTime={session.last_activity}>
            <Untrusted text={session.last_activity} /> ({age(session.last_activity, now)} ago)
          </time>
        </p>
      )}
      <p>The documents keep what the session recorded as it went. A new chat starts fresh.</p>
    </Notice>
  );
}
