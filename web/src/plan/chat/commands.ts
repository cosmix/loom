import type { PlanMessage } from "@/plan/api/schema";
import type {
  AnyAttachmentInput,
  AttachmentInput,
  ChatCommand,
  CommandResult,
  FileAttachmentInput,
} from "@/plan/api/schema-chat";
import type { connectPlanSocket } from "@/plan/api/socket";
import type { AttachmentDraft, ChatAttachment, FileDraft } from "@/plan/attachments/types";
import { repoRel } from "@/plan/files/address";
import { newId } from "@/plan/id";
import type { ReviewCommand, SuggestedPassage } from "@/plan/review/api";
import { connectionAtom, planSocketAtom } from "@/plan/state/atoms";
import type { Store } from "@/state/store";
import {
  CONNECTION_LOST,
  markInterruptPending,
  registerPending,
  rejectAllPending,
  rejectPending,
  resolvePending,
} from "./model";

export { getSessionBrief } from "@/plan/api/schema-session-brief";

type WithoutClientId<T> = T extends unknown ? Omit<T, "client_id"> : never;
export type ChatRequest = WithoutClientId<ChatCommand>;
type Request = ({ type: "chat" } & ChatRequest) | ({ type: "review" } & ReviewCommand);
type Entry = { clientId: string; request: Request };
type PlanSocket = ReturnType<typeof connectPlanSocket>;
type Incoming<Type extends PlanMessage["type"]> = Extract<PlanMessage, { type: Type }>;
type PendingCommands = {
  entries: Entry[];
  dispose: () => void;
};
const commands = new WeakMap<Store, PendingCommands>();
const NOT_CONNECTED = "Not connected; the message is kept";

export function toAttachmentInput(draft: FileDraft): FileAttachmentInput;
export function toAttachmentInput(draft: AttachmentDraft): AttachmentInput;
export function toAttachmentInput(draft: ChatAttachment): AnyAttachmentInput;
export function toAttachmentInput(draft: ChatAttachment): AnyAttachmentInput {
  // An upload goes by the id the server issued: no path, no quote.
  if (draft.kind === "file") return { kind: "file", id: draft.id, name: draft.name };
  if (draft.kind === "source") {
    return {
      kind: "source",
      path: repoRel(draft.path),
      anchor: draft.anchor,
      lines: draft.lines && [draft.lines.start, draft.lines.end],
      stage: draft.stageId,
      summary_sentence: null,
      rests_on: [],
      quote: draft.text,
      selected_hash: draft.hash,
    };
  }
  return {
    kind: "summary",
    path: repoRel(`loom/summaries/${draft.summary}.md`),
    anchor: draft.sentence,
    lines: null,
    stage: null,
    summary_sentence: draft.sentence,
    rests_on: draft.restsOn.map((ref) => `${repoRel(ref.path)}#${ref.anchor}`),
    quote: draft.text,
    selected_hash: draft.hash,
  };
}

function finish(store: Store, pending: PendingCommands, entry: Entry, reason?: string): void {
  pending.entries = pending.entries.filter((candidate) => candidate !== entry);
  if (reason !== undefined) rejectPending(store, entry.clientId, reason);
  else resolvePending(store, { client_id: entry.clientId, ok: true, error: null });
}

// socket.ts validates every frame against the shared schema before it reaches a handler.
function receiveChat(store: Store, pending: PendingCommands, message: Incoming<"chat">): void {
  if (message.frame !== "result") return;
  const entry = pending.entries.find(
    (entry) => entry.request.type === "chat" && entry.clientId === message.client_id,
  );
  if (entry)
    finish(store, pending, entry, message.ok ? undefined : (message.error ?? "Command refused"));
}

function receiveReview(store: Store, pending: PendingCommands, reply: Incoming<"review">): void {
  // Review has no client ids. Refusals and sent acknowledgements follow request order.
  const entry = pending.entries.find(({ request }) => {
    if (request.type !== "review") return false;
    if (reply.kind === "refused") return true;
    if (reply.kind !== "suggestion_status") return false;
    if (request.kind === "suggest") return reply.state === "sent";
    return (
      request.id === reply.id &&
      reply.state === (request.action === "attach" ? "attached" : "dismissed")
    );
  });
  if (entry) finish(store, pending, entry, reply.kind === "refused" ? reply.reason : undefined);
}

function receiveRefusal(store: Store, pending: PendingCommands, message: Incoming<"refused">) {
  const { command, reason } = message;
  const entry = pending.entries.find(
    ({ request }) =>
      command === "frame" ||
      command === request.type ||
      command === (request.type === "chat" ? request.command : request.kind),
  );
  if (entry) finish(store, pending, entry, reason);
}

function pendingFor(store: Store, socket: PlanSocket): PendingCommands {
  const existing = commands.get(store);
  if (existing) return existing;
  const pending: PendingCommands = { entries: [], dispose: () => {} };
  const lost = () => {
    rejectAllPending(store, CONNECTION_LOST, true);
    pending.entries = [];
    pending.dispose();
  };
  const stops = [
    socket.subscribe("chat", (message) => receiveChat(store, pending, message)),
    socket.subscribe("review", (message) => receiveReview(store, pending, message)),
    socket.subscribe("refused", (message) => receiveRefusal(store, pending, message)),
    store.sub(connectionAtom, () => {
      const { phase } = store.get(connectionAtom);
      if (phase === "reconnecting" || phase === "unauthorized") lost();
    }),
    store.sub(planSocketAtom, lost),
  ];
  pending.dispose = () => {
    stops.splice(0).forEach((stop) => stop());
    if (commands.get(store) === pending) commands.delete(store);
  };
  commands.set(store, pending);
  return pending;
}

function dispatch(store: Store, request: Request): Promise<CommandResult> {
  const socket = store.get(planSocketAtom);
  if (!socket) return Promise.reject(new Error(NOT_CONNECTED));
  const pending = pendingFor(store, socket);
  const entry = { clientId: newId(), request };
  const content =
    request.type === "chat" &&
    (request.command === "send" || request.command === "start") &&
    (request.text !== null || request.attachments.length > 0)
      ? { text: request.text ?? "", attachments: request.attachments }
      : undefined;
  const result = registerPending(store, entry.clientId, content);
  pending.entries.push(entry);
  // The server returns the unsent queue to the asking tab on interrupt, on end and on setting a
  // kickoff aside.
  if (
    request.type === "chat" &&
    (request.command === "interrupt" ||
      request.command === "end" ||
      request.command === "set_aside_kickoff")
  ) {
    markInterruptPending(store, entry.clientId);
  }
  try {
    const message = request.type === "chat" ? { ...request, client_id: entry.clientId } : request;
    if (!socket.send(message)) finish(store, pending, entry, NOT_CONNECTED);
  } catch (error) {
    pending.entries = pending.entries.filter((candidate) => candidate !== entry);
    rejectPending(store, entry.clientId, error instanceof Error ? error : new Error(String(error)));
  }
  return result.finally(() => {
    pending.entries = pending.entries.filter((candidate) => candidate !== entry);
    if (!pending.entries.length) pending.dispose();
  });
}

export function sendChatCommand(store: Store, request: ChatRequest): Promise<CommandResult> {
  return dispatch(store, { ...request, type: "chat" });
}

export function startChat(
  store: Store,
  offer: string | null = null,
  text: string | null = null,
  attachments: AnyAttachmentInput[] = [],
  feature?: string,
  plan?: string,
): Promise<CommandResult> {
  return sendChatCommand(store, {
    command: "start",
    offer,
    text,
    attachments,
    ...(feature !== undefined ? { feature } : {}),
    ...(plan !== undefined ? { plan } : {}),
  });
}

export function sendMessage(store: Store, text: string, attachments: AnyAttachmentInput[] = []) {
  return sendChatCommand(store, { command: "send", text, attachments });
}

export function withdrawMessage(store: Store, queueId: string) {
  return sendChatCommand(store, { command: "withdraw", queue_id: queueId });
}

export function answerPermission(
  store: Store,
  requestId: string,
  allow: boolean,
  note: string | null = null,
) {
  return sendChatCommand(store, { command: "permission", request_id: requestId, allow, note });
}

/** "Allow this session" on a WebFetch card. */
export function allowSessionFetches(store: Store, requestId: string) {
  return sendChatCommand(store, { command: "allow_session_fetches", request_id: requestId });
}

/** "Always allow <host>" on a WebFetch card: the server reads the host from the request. */
export function allowFetchHost(store: Store, requestId: string) {
  return sendChatCommand(store, { command: "allow_fetch_host", request_id: requestId });
}

/** "Ask again": the session's web fetches show a card again. */
export function askFetchesAgain(store: Store) {
  return sendChatCommand(store, { command: "ask_fetches_again" });
}

export function setAsideKickoff(store: Store, session: string) {
  return sendChatCommand(store, { command: "set_aside_kickoff", session });
}

export function continueKickoff(store: Store, session: string) {
  return sendChatCommand(store, { command: "continue_kickoff", session });
}

export function discardKickoff(store: Store, session: string) {
  return sendChatCommand(store, { command: "discard_kickoff", session });
}

export function interruptChat(store: Store) {
  return sendChatCommand(store, { command: "interrupt" });
}

export function endChat(store: Store) {
  return sendChatCommand(store, { command: "end" });
}

export function sendReviewCommand(store: Store, request: ReviewCommand): Promise<CommandResult> {
  return dispatch(store, { ...request, type: "review" });
}

export function suggest(store: Store, passage: SuggestedPassage, note: string) {
  return sendReviewCommand(store, { kind: "suggest", passage, note });
}

export function answerSuggestion(store: Store, id: string, action: "attach" | "dismiss") {
  return sendReviewCommand(store, { kind: "suggestion_answer", id, action });
}
