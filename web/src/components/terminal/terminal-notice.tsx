import { useNow } from "@/components/hooks/use-now";
import type { TerminalState } from "@/components/terminal/use-terminal";
import { formatElapsed } from "@/lib/format";

/// What the well says when there is no screen to show, or a stamp over the
/// last one: waiting, refused, ended, dropped.
export function Notice({
  state,
  alive,
  terminals,
  retry,
}: {
  state: TerminalState;
  alive: boolean;
  terminals: boolean;
  retry: () => void;
}) {
  switch (state.phase) {
    case "waiting":
      return (
        <div className="terminal-notice" role="status">
          <span className="line">
            <span className="terminal-dot dot-live" aria-hidden="true" />
            waiting for the session's tmux server
          </span>
          <span>{[state.reason, "retrying in 2s"].filter(Boolean).join(" · ")}</span>
        </div>
      );
    case "refused":
      return (
        <div className="terminal-notice" role="alert">
          <span className="line">{refusedText(state.reason, terminals)}</span>
        </div>
      );
    case "ended":
      return (
        <Stamp since={state.since} word="ended" action={alive ? "Reconnect" : null} retry={retry} />
      );
    case "dropped":
      return <Stamp since={null} word="connection dropped" action="Retry" retry={retry} />;
    default:
      return null;
  }
}

/// The server's refusals that reach the browser as a handshake failure carry
/// no status; the two causes are told apart by whether terminals are on.
function refusedText(reason: string | undefined, terminals: boolean): string {
  if (reason === "handshake refused") {
    return terminals
      ? "Dashboard cookie missing — open the tokenized URL printed when the dashboard started"
      : "Start the dashboard with loom status --web --terminals";
  }
  return reason ?? "refused";
}

function Stamp({
  since,
  word,
  action,
  retry,
}: {
  since: number | null;
  word: string;
  action: string | null;
  retry: () => void;
}) {
  const now = useNow();
  const ago =
    since === null ? "" : ` · ${formatElapsed(Math.max(0, Math.floor((now - since) / 1000)))} ago`;
  return (
    <div className="terminal-stamp" role="status">
      <span className="terminal-stamp-word">
        <span className="terminal-dot" aria-hidden="true" />
        {word}
        {ago}
      </span>
      {action && (
        <button type="button" onClick={retry}>
          {action}
        </button>
      )}
    </div>
  );
}
