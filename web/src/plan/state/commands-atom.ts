import { atom } from "jotai/vanilla";
import { unwrap } from "jotai/vanilla/utils";

import { apiGet } from "@/plan/api/client";
import { commandsViewSchema, type CommandsView } from "@/plan/api/schema-session-commands";
import { sessionAtom } from "@/plan/chat/model";

/** A session's command list; `session` is null while the chat has no session. */
export type SessionCommands = CommandsView & { session: string | null };
/** The fields of a `commands` frame, without its tag. */
type CommandsFrame = CommandsView & { session: string };

const ROUTE = "/api/session/commands";
/** What is known before the route answers: nothing is listed, so every slash message waits. */
const LOADING: SessionCommands = {
  session: null,
  state: "loading",
  generation: null,
  reason: null,
  entries: [],
};

/**
 * `view` as the server's loading table: the built-in rows alone, which is what it checks a chat
 * start against while no session's list exists (`CommandTable::loading`).
 */
export function loadingView(view: CommandsView): CommandsView {
  const entries = view.entries.filter((entry) => entry.source === "built-in");
  return { state: "loading", generation: null, reason: null, entries };
}

/** The chat session's id alone, so a snapshot that keeps its session asks for nothing. */
const sessionIdAtom = atom((get) => get(sessionAtom)?.id ?? null);

/**
 * A ready list's generation. A session's list goes from loading to ready and then only up in
 * generation, so a list that is not ready ranks under every ready one.
 */
function rank(view: CommandsView): number {
  return view.state === "ready" ? (view.generation ?? 0) : -1;
}

const frameAtom = atom<CommandsFrame | null>(null);
/** The hellos the plan socket has had: none until the page is connected. */
const connectionsAtom = atom(0);

/**
 * The socket's setter for a `commands` frame. A frame for a session other than the chat's is
 * ignored, and a lower generation never replaces a higher one of the same session.
 */
export const commandsFrameAtom = atom(null, (get, set, frame: CommandsFrame) => {
  const session = get(sessionIdAtom);
  if (session !== null && frame.session !== session) return;
  set(frameAtom, (held) =>
    held !== null && held.session === frame.session && rank(frame) < rank(held) ? held : frame,
  );
});

/**
 * The socket's setter for a hello: the route is asked again, and the frame held from the
 * connection before goes (a new connection may be a restarted server, whose generations start
 * over). It bumps with `connectionEpochAtom` and once more on the first hello, which that epoch
 * does not count.
 */
export const commandsResyncAtom = atom(null, (_get, set) => {
  set(frameAtom, null);
  set(connectionsAtom, (count) => count + 1);
});

/**
 * The route's answer for the chat session (the native rows alone, as `loading`, while there is
 * none), asked again on a session change and on every hello. A store whose socket never said
 * hello asks for nothing: a help scene and a test harness without a server fetch nothing.
 */
const answerAtom = atom((get, { signal }): Promise<SessionCommands> | null => {
  const session = get(sessionIdAtom);
  if (get(connectionsAtom) === 0) return null;
  const query = session === null ? "" : `?session=${encodeURIComponent(session)}`;
  return apiGet(`${ROUTE}${query}`, commandsViewSchema, signal).then((result): SessionCommands =>
    result.ok
      ? { ...result.value, session }
      : { ...LOADING, session, state: "unavailable", reason: result.message },
  );
});

/** The last answer stays while the next one is on its way, so a resync does not empty the list. */
const answeredAtom = unwrap(answerAtom, (previous) => previous ?? null);

/**
 * The command list of the chat session: the newer of the route's answer and the socket's last
 * frame. The frame wins unless the answer is a ready list of a higher generation.
 */
export const commandsAtom = atom((get): SessionCommands => {
  const session = get(sessionIdAtom);
  const frame = get(frameAtom);
  const answer = get(answeredAtom);
  const pushed = frame !== null && frame.session === session ? frame : null;
  const pulled = answer !== null && answer.session === session ? answer : null;
  if (pushed === null) return pulled ?? (session === null ? LOADING : { ...LOADING, session });
  return pulled !== null && rank(pulled) > rank(pushed) ? pulled : pushed;
});
