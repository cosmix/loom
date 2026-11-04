import { useAtomValue, useStore } from "jotai";
import { useEffect, useId, useRef, useState } from "react";
import { Link, useNavigate } from "react-router";

import {
  getKickoffDraft,
  type KickoffDraftDetail,
  type KickoffDraftView,
} from "@/plan/api/schema-kickoff";
import { continueKickoff, discardKickoff, setAsideKickoff } from "@/plan/chat/commands";
import { sessionAtom } from "@/plan/chat/model";
import { Untrusted } from "@/plan/chat/visible-controls";
import { draftTitle, kickoffHref, waitForKickoffSession } from "@/plan/new/kickoff-state";
import { KIND_UNDECIDED, KIND_WORD } from "@/plan/new/kickoff-head";
import { DecisionsRow, ProblemRow, QuestionsRow } from "@/plan/new/taking-shape";
import { authorBlockReasonAtom, canAuthorAtom, kickoffDraftsAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { LoadState } from "@/plan/ui/load-state";

type Load =
  | { status: "loading" }
  | { status: "missing" }
  | { status: "error"; message: string }
  | { status: "ready"; detail: KickoffDraftDetail };

/**
 * One draft command at a time, gated like the proposal's actions: `canAuthorAtom` through
 * `DisabledReason`. `run` resolves true when the server accepted the command.
 */
function useDraftCommand() {
  const store = useStore();
  const canAuthor = useAtomValue(canAuthorAtom);
  const blockReason = useAtomValue(authorBlockReasonAtom);
  const [pending, setPending] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const reason = !canAuthor
    ? (blockReason ?? "Authoring is unavailable")
    : pending
      ? "Waiting for the server"
      : null;
  const run = async (action: () => Promise<unknown>): Promise<boolean> => {
    if (!store.get(canAuthorAtom) || pending) return false;
    setPending(true);
    setStatus(null);
    try {
      await action();
      return true;
    } catch (cause) {
      setStatus(cause instanceof Error ? cause.message : String(cause));
      return false;
    } finally {
      setPending(false);
    }
  };
  return { reason, status, run };
}

function CommandStatus({ status }: { status: string | null }) {
  if (status === null) return null;
  return (
    <p role="status" className="plan-fine">
      <Untrusted text={status} />
    </p>
  );
}

/**
 * Above the empty New work form while a kickoff is running: the way back into it, or a way to put
 * it down. Setting it aside ends the session and keeps the draft, so the form is usable again.
 */
export function OpenDraftNotice({ draft }: { draft: KickoffDraftView }) {
  const store = useStore();
  const { reason, status, run } = useDraftCommand();
  return (
    <div className="kickoff-open">
      <p className="kickoff-open__text">
        <Untrusted text={draftTitle(draft)} /> is still open
      </p>
      <div className="kickoff-open__actions">
        <Link to={kickoffHref(draft.session)} data-look="secondary" className="plan-button">
          Continue
        </Link>
        <DisabledReason
          reason={reason}
          look="quiet"
          onClick={() => void run(() => setAsideKickoff(store, draft.session))}
        >
          Set it aside
        </DisabledReason>
      </div>
      <CommandStatus status={status} />
    </div>
  );
}

function useDraftDetail(id: string): Load {
  // The list says when the server stopped running this kickoff; the detail fetched while it ran
  // still reads live, so the change is the cue to fetch again.
  const live = useAtomValue(kickoffDraftsAtom).find((entry) => entry.session === id)?.live;
  const [loaded, setLoaded] = useState<{ id: string; state: Load } | null>(null);
  // Only turning not running refetches. Continuing a draft deletes it (live becomes undefined); a
  // fetch then would 404 under the actions that are waiting for the new session.
  const [stops, setStops] = useState(0);
  const before = useRef(live);
  useEffect(() => {
    if (before.current !== false && live === false) setStops((count) => count + 1);
    before.current = live;
  }, [live]);
  useEffect(() => {
    let active = true;
    void getKickoffDraft(id).then((result) => {
      if (!active) return;
      const state: Load = result.ok
        ? { status: "ready", detail: result.value }
        : result.status === 404
          ? { status: "missing" }
          : { status: "error", message: result.message };
      setLoaded({ id, state });
    });
    return () => {
      active = false;
    };
  }, [id, stops]);
  return loaded?.id === id ? loaded.state : { status: "loading" };
}

function DraftActions({ session }: { session: string }) {
  const store = useStore();
  const navigate = useNavigate();
  const { reason, status, run } = useDraftCommand();
  const [confirming, setConfirming] = useState(false);
  const [late, setLate] = useState(false);
  const wait = useRef<AbortController | null>(null);
  useEffect(() => () => wait.current?.abort(), []);
  const resume = async () => {
    const previous = store.get(sessionAtom)?.id ?? null;
    if (!(await run(() => continueKickoff(store, session)))) return;
    // The route moves this tab to the new conversation once its session is recorded.
    wait.current = new AbortController();
    setLate(false);
    void waitForKickoffSession(store, previous, wait.current.signal, () => setLate(true));
  };
  const discard = async () => {
    if (await run(() => discardKickoff(store, session))) void navigate("/new", { replace: true });
  };
  return (
    <div className="kickoff-draft__actions">
      {confirming ? (
        <div role="group" aria-label="Confirm discard" className="kickoff-draft__confirm">
          <p>Discard this draft? It cannot be brought back.</p>
          <DisabledReason reason={reason} onClick={() => void discard()}>
            Discard for good
          </DisabledReason>
          <DisabledReason reason={null} look="quiet" onClick={() => setConfirming(false)}>
            Keep it
          </DisabledReason>
        </div>
      ) : (
        <>
          <DisabledReason reason={reason} look="primary" onClick={() => void resume()}>
            Continue this draft
          </DisabledReason>
          <DisabledReason reason={reason} look="quiet" onClick={() => setConfirming(true)}>
            Discard
          </DisabledReason>
        </>
      )}
      {late && <p className="plan-fine">The conversation did not start. Please try again.</p>}
      <CommandStatus status={status} />
    </div>
  );
}

function DraftMessages({ messages }: { messages: string[] }) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="kickoff-draft__messages">
      <h2 id={headingId} className="plan-section-title">
        Your messages
      </h2>
      {messages.length === 0 ? (
        <p className="kickoff-muted">Nothing typed yet</p>
      ) : (
        <ol className="kickoff-draft__list">
          {messages.map((text, index) => (
            <li key={`${index}:${text}`} className="whitespace-pre-wrap">
              <Untrusted text={text} />
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}

function DraftShape({ detail }: { detail: KickoffDraftDetail }) {
  const { draft, shape } = detail;
  return (
    <div className="kickoff-shape kickoff-draft__shape font-sans">
      <ProblemRow problem={shape?.problem || draft.problem} />
      <DecisionsRow decisions={shape?.decisions ?? []} />
      <QuestionsRow questions={shape?.questions ?? []} />
    </div>
  );
}

/**
 * One short document in the kickoff's page grid: its head where the conversation's head sits, the
 * way on, then what was settled and what the user typed.
 */
function DraftBody({ detail }: { detail: KickoffDraftDetail }) {
  const headingId = useId();
  const { draft } = detail;
  const kind = draft.size === null ? KIND_UNDECIDED : KIND_WORD[draft.size];
  return (
    <div className="kickoff-page kickoff-draft" data-state="draft">
      <section aria-labelledby={headingId} className="kickoff-main">
        <header className="kickoff-title">
          <p className="plan-eyebrow">{kind}</p>
          <div className="kickoff-title__row">
            <h1 id={headingId} className="plan-page-title">
              <Untrusted text={draftTitle(draft)} />
            </h1>
            <span className="kickoff-pill plan-status">
              <span aria-hidden="true" className="kickoff-pill__glyph">
                ○
              </span>
              Not running
            </span>
          </div>
        </header>
        <DraftActions session={draft.session} />
        <DraftShape detail={detail} />
        <DraftMessages messages={detail.messages} />
      </section>
    </div>
  );
}

/**
 * `/new?session=<id>` for a kickoff that is not running: what was settled and what the user typed,
 * read-only, with the two ways on. A running kickoff shows its conversation instead
 * (`KickoffRoute`).
 */
export function DraftView({ id }: { id: string }) {
  const load = useDraftDetail(id);
  if (load.status === "loading") return <LoadState state="loading" title="Opening the draft" />;
  if (load.status === "ready" && load.detail.draft.live) {
    // The server runs it; its conversation arrives with the next snapshot.
    return <LoadState state="loading" title="Opening the conversation" />;
  }
  if (load.status === "ready") return <DraftBody detail={load.detail} />;
  return (
    <>
      <LoadState
        state="error"
        title={load.status === "missing" ? "That draft is gone" : "The draft did not load"}
        message={load.status === "error" ? load.message : undefined}
      />
      <Link to="/new" data-look="secondary" className="plan-button">
        Start new work
      </Link>
    </>
  );
}
