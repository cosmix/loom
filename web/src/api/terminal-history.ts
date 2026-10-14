/// Where the dashboard host serves a session's saved terminal history
/// (`loom/src/commands/status/web/terminal/history.rs`).
const HISTORY_PATH = "/api/terminal-history/";

/// Fetches a session's saved terminal history, or `null` when it has none.
export type HistoryLoader = (session: string) => Promise<Uint8Array | null>;

/// The raw bytes of the history file: terminal text with its colors, for an
/// emulator to replay. A refusal or a server error rejects.
export const fetchTerminalHistory: HistoryLoader = async (session) => {
  const response = await fetch(`${HISTORY_PATH}${encodeURIComponent(session)}`, {
    credentials: "same-origin",
  });
  if (response.status === 404) return null;
  if (!response.ok) throw new Error(`terminal history request failed: ${response.status}`);
  return new Uint8Array(await response.arrayBuffer());
};
