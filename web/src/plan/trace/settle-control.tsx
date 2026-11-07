import { cn } from "cn";
import { useAtomValue } from "jotai/react";
import { ChevronDown } from "lucide-react";
import { Popover as PopoverPrimitive } from "radix-ui";
import { useId, useRef, useState, type JSX, type RefObject } from "react";

import type { TraceDoc } from "@/plan/api/schema-trace";
import { Untrusted } from "@/plan/chat/visible-controls";
import { Popover } from "@/plan/header/popover";
import { authorBlockReasonAtom } from "@/plan/state/atoms";
import { HEADING_BUTTON } from "@/plan/ui/doc-heading";

import { changeTarget, settleWords } from "./doc-model";
import { docOfPath, shownSettle } from "./model";
import { SettleConfirm } from "./settle-confirm";
import { useSettle } from "./settle-flow";
import { SettleGlyph } from "./settle-glyph";
import { SettleMenu } from "./settle-menu";
import { SettleSurface, useNarrow } from "./settle-surface";
import { ChangesSinceSettled } from "./settled-changes";
import { useTrace } from "./state";

import "./trace.css";

const CONFIRM_TITLE = "Settle with open items?";

type Flow = ReturnType<typeof useSettle>;

/** The one button: its label is the state, and a settled or moved state opens a menu from it. */
function SettleButton(props: {
  doc: TraceDoc;
  buttonRef: RefObject<HTMLButtonElement | null>;
  reason: string | null;
  reasonId: string;
  open: boolean;
  onPress: () => void;
}): JSX.Element {
  const { doc, buttonRef, reason, reasonId, open, onPress } = props;
  const words = settleWords(doc.settle, doc.role);
  const menu = doc.settle !== "draft";
  // The anchor places the confirmation under the button, which no Radix trigger opens.
  return (
    <PopoverPrimitive.Anchor asChild>
      <button
        ref={buttonRef}
        type="button"
        {...HEADING_BUTTON}
        className={cn(HEADING_BUTTON.className, "trace-settle-button")}
        data-settle={doc.settle}
        aria-disabled={reason !== null || undefined}
        aria-describedby={reason !== null ? reasonId : undefined}
        aria-haspopup={menu ? "dialog" : undefined}
        aria-expanded={menu ? open : undefined}
        onClick={onPress}
      >
        <SettleGlyph state={doc.settle} />
        {words.button}
        {menu && <ChevronDown aria-hidden="true" />}
      </button>
    </PopoverPrimitive.Anchor>
  );
}

/** The confirmation of an `unresolved` answer, else the menu the button opened. */
function SettlePanel(props: {
  feature: string;
  doc: TraceDoc;
  flow: Flow;
  menuOpen: boolean;
  run: (go: () => Promise<boolean>) => void;
  closeMenu: () => void;
}): JSX.Element | null {
  const { feature, doc, flow, menuOpen, run, closeMenu } = props;
  const { phase } = flow;
  if (phase.kind === "confirm") {
    return (
      <SettleConfirm
        feature={feature}
        path={doc.path}
        myCalls={phase.myCalls}
        openQuestions={phase.openQuestions}
        onConfirm={() => run(flow.confirm)}
        onCancel={flow.dismiss}
      />
    );
  }
  if (!menuOpen) return null;
  const choose = (go: () => Promise<boolean>) => {
    closeMenu();
    run(go);
  };
  return (
    <SettleMenu
      moved={doc.settle !== "settled"}
      onSettle={() => choose(flow.settle)}
      onUnsettle={() => choose(flow.unsettle)}
    />
  );
}

/** The menu's state, the focus that returns to the button, and the runner of a flow step. */
function useSettleMenu(flow: Flow, button: RefObject<HTMLButtonElement | null>) {
  const [menuOpen, setMenuOpen] = useState(false);
  const confirming = flow.phase.kind === "confirm";
  const focusButton = () => button.current?.focus();
  return {
    menuOpen,
    open: menuOpen || confirming,
    focusButton,
    closeMenu: () => setMenuOpen(false),
    toggleMenu: () => setMenuOpen((open) => !open),
    // Closing from outside or with Escape also drops a pending confirmation.
    close: () => {
      setMenuOpen(false);
      if (confirming) flow.dismiss();
    },
    // A written line turns the button's words; focus stays on the same button.
    run: (go: () => Promise<boolean>) =>
      void go().then((written) => {
        if (written) focusButton();
      }),
  };
}

/** What stands beside the button: why it is disabled, the way to see what changed, a notice. */
function SettleAside(props: {
  feature: string;
  doc: TraceDoc;
  flow: Flow;
  reason: string | null;
  reasonId: string;
}): JSX.Element {
  const { feature, doc, flow, reason, reasonId } = props;
  const { trace } = useTrace(feature);
  const target = trace ? changeTarget(trace, doc) : null;
  return (
    <>
      {reason !== null && (
        <span id={reasonId} className="plan-fine text-muted-foreground">
          <Untrusted text={reason} />
        </span>
      )}
      {target && (
        <ChangesSinceSettled settled={target.settled} compare={target.compare} look="button" />
      )}
      {flow.phase.kind === "notice" && (
        <span role="alert" className="trace-settle-note">
          <Untrusted text={flow.phase.message} />
        </span>
      )}
    </>
  );
}

function SettleInteractive(props: { feature: string; doc: TraceDoc; hash: string }): JSX.Element {
  const { feature, doc, hash } = props;
  const flow = useSettle(feature, doc.path, hash);
  const blocked = useAtomValue(authorBlockReasonAtom);
  const narrow = useNarrow();
  const reasonId = useId();
  const button = useRef<HTMLButtonElement>(null);
  const menu = useSettleMenu(flow, button);
  const confirming = flow.phase.kind === "confirm";
  const reason = flow.phase.kind === "working" ? "Working" : blocked;
  const press = () => {
    if (reason !== null) return;
    if (doc.settle === "draft") menu.run(flow.settle);
    else menu.toggleMenu();
  };
  return (
    <div className="trace-settle-control">
      <Popover open={menu.open && !narrow} onOpenChange={(next) => !next && menu.close()}>
        <SettleButton
          doc={doc}
          buttonRef={button}
          reason={reason}
          reasonId={reasonId}
          open={menu.open}
          onPress={press}
        />
        <SettleSurface
          open={menu.open}
          narrow={narrow}
          label={confirming ? CONFIRM_TITLE : settleWords(doc.settle, doc.role).button}
          showTitle={confirming}
          onClose={menu.close}
          returnFocus={menu.focusButton}
          button={button}
        >
          <SettlePanel
            feature={feature}
            doc={doc}
            flow={flow}
            menuOpen={menu.menuOpen}
            run={menu.run}
            closeMenu={menu.closeMenu}
          />
        </SettleSurface>
      </Popover>
      <SettleAside feature={feature} doc={doc} flow={flow} reason={reason} reasonId={reasonId} />
    </div>
  );
}

function SettleBody(props: {
  feature: string;
  doc: TraceDoc;
  hash: string | null;
  locked: boolean | undefined;
}): JSX.Element {
  const { feature, doc, hash, locked } = props;
  // A locked document, and one whose hash has not loaded, show the state alone.
  if (locked || hash === null) {
    return (
      <span className="trace-settle-state" data-settle={doc.settle}>
        <SettleGlyph state={doc.settle} />
        {settleWords(doc.settle, doc.role).chip}
      </span>
    );
  }
  return <SettleInteractive feature={feature} doc={doc} hash={hash} />;
}

/** What `SettleControl` takes as `locked` for an opened document; undefined until one is open. */
export function docIsLocked(
  doc: { lock: unknown; editable: boolean } | null | undefined,
): boolean | undefined {
  return doc ? doc.lock !== null || !doc.editable : undefined;
}

/**
 * A document's settle state as one button, whose label is the state: "Mark settled" for a draft,
 * and for a settled or moved one the state with a menu of what the reader may do about it. `path`
 * is the repository path, `hash` the opened document's save hash and `locked` the server's answer
 * for it (`lock !== null` or not `editable`). Renders nothing for a path the trace does not know (a
 * brief, a file outside the feature's documents) or for done work, which is never settled
 * (`shownSettle`: a locked plan, a done feature's vision and metaplan).
 */
export function SettleControl(props: {
  feature: string;
  path: string;
  hash: string | null;
  locked?: boolean;
}): JSX.Element | null {
  const { trace } = useTrace(props.feature);
  const doc = trace ? docOfPath(trace, props.path) : undefined;
  if (!doc || shownSettle(doc) === null) return null;
  // Keyed by document: a phase or lookup in flight for one must not show under the next.
  return (
    <SettleBody
      key={doc.path}
      feature={props.feature}
      doc={doc}
      hash={props.hash}
      locked={props.locked}
    />
  );
}
