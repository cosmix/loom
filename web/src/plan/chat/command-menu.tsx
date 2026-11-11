import { useAtomValue, useStore } from "jotai";
import { SquareTerminal } from "lucide-react";
import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type RefObject,
  type TextareaHTMLAttributes,
} from "react";

import type { CommandEntry } from "@/plan/api/schema-session-commands";
import { useFloatingPlacement } from "@/plan/attachments/floating-position";
import { commandsAtom, type SessionCommands } from "@/plan/state/commands-atom";
import { BusyText } from "@/plan/ui/busy-text";
import { focusComposerAtom } from "./composer-hooks";
import { draftAtom } from "./drafts";
import {
  matchingCommands,
  pendingArgumentHint,
  terminalOnlyReason,
  typedCommandName,
  withCommand,
} from "./message-problem";
import { Untrusted, visibleControls } from "./visible-controls";
import "./command-menu.css";

/** Kept between the list and the viewport's edge, with the gap to the composer box. */
const EDGE = 14;
/** The list never gets shorter than about two entries. */
const MIN_HEIGHT = 112;

type MenuKeys = {
  entry: CommandEntry | undefined;
  move: (by: number) => void;
  pick: (entry: CommandEntry) => void;
  dismiss: () => void;
};

/**
 * A key pressed in the text while the list is open. True when the list took it, so the composer
 * must not read it as its own (Enter must not send). Escape is taken at the window (`OpenMenu`).
 */
function menuKey(event: KeyboardEvent<HTMLTextAreaElement>, keys: MenuKeys): boolean {
  if (event.nativeEvent.isComposing || event.altKey || event.ctrlKey || event.metaKey) return false;
  const { entry } = keys;
  switch (event.key) {
    case "ArrowDown":
    case "ArrowUp":
      event.preventDefault();
      keys.move(event.key === "ArrowDown" ? 1 : -1);
      return true;
    case "Enter":
      if (event.shiftKey || entry === undefined) return false;
      event.preventDefault();
      keys.pick(entry);
      return true;
    case "Tab":
      // With nothing to insert, Tab closes the list and moves on: the list never traps focus.
      if (event.shiftKey || entry === undefined || terminalOnlyReason(entry) !== null) {
        keys.dismiss();
        return false;
      }
      event.preventDefault();
      keys.pick(entry);
      return true;
    default:
      return false;
  }
}

/**
 * The text box's combobox attributes. They are set only while the text is a slash name or the
 * list is open: with plain text the box stays the multi-line text box it is.
 */
function fieldProps(
  combo: boolean,
  open: boolean,
  listId: string,
  activeId: string | undefined,
): TextareaHTMLAttributes<HTMLTextAreaElement> {
  if (!combo) return {};
  return {
    role: "combobox",
    "aria-expanded": open,
    "aria-autocomplete": "list",
    "aria-haspopup": "listbox",
    "aria-controls": open ? listId : undefined,
    "aria-activedescendant": open ? activeId : undefined,
  };
}

/** The entries the text keeps and the highlighted one: the first, whenever the typed name changes. */
function useMenuEntries(text: string, commands: SessionCommands) {
  const [cursor, setCursor] = useState({ query: "", index: 0 });
  const typed = typedCommandName(text);
  const query = typed ?? "";
  const entries = matchingCommands(commands, query);
  const last = entries.length - 1;
  const active = cursor.query === query ? Math.min(cursor.index, last) : 0;
  // The pointer reports every move over an entry: the same highlight is not a new state.
  const activate = (wanted: number) => {
    const index = Math.max(0, Math.min(last, wanted));
    setCursor((held) => (held.query === query && held.index === index ? held : { query, index }));
  };
  return { typed, query, entries, active, activate };
}

/**
 * The command list of the composer: open while the text is a `/` and a name, or after a press
 * of Commands; closed by Escape, a press outside, or a pick. Focus stays in the text box, which
 * takes the list's keys through `onKeyDown`.
 */
export function useCommandMenu() {
  const store = useStore();
  const { text } = useAtomValue(draftAtom);
  const commands = useAtomValue(commandsAtom);
  const id = useId();
  const listId = `${id}-list`;
  const optionId = (index: number) => `${id}-option-${index}`;
  const { typed, query, entries, active, activate } = useMenuEntries(text, commands);
  // Commands, Escape and a pick decide for the text they met; the next edit decides again.
  const [choice, setChoice] = useState<{ text: string; open: boolean } | null>(null);
  const open = choice !== null && choice.text === text ? choice.open : typed !== null;
  const dismiss = useCallback(
    () => setChoice({ text: store.get(draftAtom).text, open: false }),
    [store],
  );
  const pick = (entry: CommandEntry) => {
    if (terminalOnlyReason(entry) !== null) return;
    const next = withCommand(text, entry.name);
    store.set(draftAtom, (draft) => ({ ...draft, text: next }));
    setChoice({ text: next, open: false });
    store.set(focusComposerAtom, true);
  };
  const toggle = () => {
    setChoice({ text, open: !open });
    store.set(focusComposerAtom, true);
  };
  const entry = entries[active];
  const keys: MenuKeys = { entry, move: (by) => activate(active + by), pick, dismiss };
  const list = { open, query, entries, active, commands, listId, optionId, activate, pick };
  return {
    ...list,
    text,
    dismiss,
    toggle,
    hintId: `${id}-hint`,
    hint: pendingArgumentHint(text, commands),
    field: fieldProps(open || typed !== null, open, listId, entry && optionId(active)),
    onKeyDown: (event: KeyboardEvent<HTMLTextAreaElement>) => open && menuKey(event, keys),
  };
}

export type CommandMenuState = ReturnType<typeof useCommandMenu>;

type CommandHintProps = { menu: CommandMenuState; field: RefObject<HTMLTextAreaElement | null> };

/**
 * The picked command's argument hint, drawn after the typed `/name ` where its arguments go: a
 * placeholder for the rest of the line. It is the text box's description, and it is cut at the
 * box's edge so it never runs under the tools beside it.
 */
export function CommandHint({ menu, field }: CommandHintProps) {
  const ghost = useRef<HTMLSpanElement>(null);
  useLayoutEffect(() => {
    if (ghost.current && field.current) {
      ghost.current.style.maxWidth = `${field.current.clientWidth}px`;
    }
  });
  if (menu.hint === null) return null;
  return (
    <span ref={ghost} id={menu.hintId} className="chat-command-ghost">
      <span aria-hidden="true">{menu.text}</span>
      <Untrusted text={menu.hint} />
    </span>
  );
}

type CommandOptionProps = {
  entry: CommandEntry;
  id: string;
  active: boolean;
  onActivate: () => void;
  onPick: () => void;
};

/**
 * One entry: its name and argument hint, its source, then why it is blocked and what it does.
 * The spaces between the parts are text nodes: an accessible name trims each span's own text.
 */
function CommandOption({ entry, id, active, onActivate, onPick }: CommandOptionProps) {
  const blocked = terminalOnlyReason(entry);
  return (
    <li
      id={id}
      role="option"
      aria-selected={active}
      aria-disabled={blocked !== null || undefined}
      className="chat-command"
      onPointerMove={onActivate}
      onClick={onPick}
    >
      <span className="chat-command-head">
        <span className="chat-command-name">
          /<Untrusted text={entry.name} />
        </span>{" "}
        {entry.argument_hint !== "" && (
          <span className="chat-command-args">
            <Untrusted text={entry.argument_hint} />
          </span>
        )}
      </span>{" "}
      <span className="chat-command-source">{entry.source}</span>{" "}
      <span className="chat-command-about" title={visibleControls(entry.description)}>
        {blocked !== null && (
          <span className="chat-command-blocked">
            <SquareTerminal aria-hidden="true" />
            Terminal only · <Untrusted text={blocked} />
          </span>
        )}{" "}
        <span className="chat-command-description">
          <Untrusted text={entry.description} />
        </span>
      </span>
    </li>
  );
}

/** The line over the entries while the session's own list is not there, or null when it is. */
function MenuStatus({ id, commands }: { id: string; commands: SessionCommands }) {
  if (commands.state === "ready") return null;
  if (commands.state === "loading") {
    return (
      <p id={id} className="chat-commands-status">
        {commands.session === null ? (
          "A chat lists its skills and plugin commands once it starts"
        ) : (
          <BusyText>{"Loading this session's commands"}</BusyText>
        )}
      </p>
    );
  }
  return (
    <p id={id} className="chat-commands-status">
      <span aria-hidden="true" className="chat-commands-warning">
        △
      </span>{" "}
      {"This session's command list is unavailable"}
      {commands.reason !== null && ": "}
      {commands.reason !== null && <Untrusted text={commands.reason} />}
    </p>
  );
}

/** The keys, unspoken: the list's roles say the same to assistive technology. */
const MENU_KEYS = (
  <p className="chat-commands-foot" aria-hidden="true">
    <span>
      <kbd>↑</kbd> <kbd>↓</kbd> choose
    </span>
    <span>
      <kbd>↵</kbd> <kbd>Tab</kbd> insert
    </span>
    <span>
      <kbd>Esc</kbd> close
    </span>
  </p>
);

/**
 * The anchor's box in the viewport. `useFloatingPlacement` places on render only, so the box is
 * measured after every commit (the composer grows with its text) and on any scroll or resize.
 */
function useAnchorRect(anchor: RefObject<HTMLElement | null>): DOMRect | null {
  const [rect, setRect] = useState<DOMRect | null>(null);
  const held = useRef("");
  useLayoutEffect(() => {
    const measure = () => {
      const next = anchor.current?.getBoundingClientRect() ?? null;
      const box = next === null ? "" : [next.top, next.left, next.width, next.height].join();
      if (box === held.current) return;
      held.current = box;
      setRect(next);
    };
    measure();
    window.addEventListener("resize", measure);
    document.addEventListener("scroll", measure, true);
    return () => {
      window.removeEventListener("resize", measure);
      document.removeEventListener("scroll", measure, true);
    };
  });
  return rect;
}

/** Keeps the highlighted option inside the list's own scroll box. */
function useActiveInView(list: RefObject<HTMLUListElement | null>, active: number, query: string) {
  useLayoutEffect(() => {
    const box = list.current;
    const option = box?.children[active];
    if (!box || !(option instanceof HTMLElement)) return;
    const end = option.offsetTop + option.offsetHeight;
    if (option.offsetTop < box.scrollTop) box.scrollTop = option.offsetTop;
    else if (end > box.scrollTop + box.clientHeight) box.scrollTop = end - box.clientHeight;
  }, [list, active, query]);
}

/** The list at its place: under the composer box when there is room, else over it. */
function PlacedMenu({ menu, rect }: { menu: CommandMenuState; rect: DOMRect }) {
  const { ref, placement } = useFloatingPlacement(rect);
  const list = useRef<HTMLUListElement>(null);
  useActiveInView(list, menu.active, menu.query);
  const room = Math.max(rect.top, window.innerHeight - rect.bottom) - EDGE;
  const statusId = `${menu.listId}-status`;
  return (
    <div
      ref={ref}
      className="chat-commands"
      hidden={placement === null}
      style={{ ...placement, width: rect.width, maxHeight: Math.max(MIN_HEIGHT, room) }}
      // Keep the focus in the text box while the list is pressed.
      onMouseDown={(event) => event.preventDefault()}
    >
      <MenuStatus id={statusId} commands={menu.commands} />
      <ul
        ref={list}
        id={menu.listId}
        role="listbox"
        aria-label="Commands"
        aria-describedby={menu.commands.state === "ready" ? undefined : statusId}
        className="chat-commands-list"
      >
        {menu.entries.map((entry, index) => (
          <CommandOption
            key={entry.name}
            entry={entry}
            id={menu.optionId(index)}
            active={index === menu.active}
            onActivate={() => menu.activate(index)}
            onPick={() => menu.pick(entry)}
          />
        ))}
      </ul>
      {menu.entries.length === 0 && menu.query !== "" && (
        <p className="chat-commands-empty">
          No command matches <Untrusted text={`/${menu.query}`} />
        </p>
      )}
      {MENU_KEYS}
    </div>
  );
}

type CommandMenuProps = {
  menu: CommandMenuState;
  /** The composer box: the list hangs from it and a press inside it leaves the list open. */
  anchor: RefObject<HTMLElement | null>;
};

/** The open list: measured against its anchor, and closed by Escape or a press outside it. */
function OpenMenu({ menu, anchor }: CommandMenuProps) {
  const rect = useAnchorRect(anchor);
  const { dismiss } = menu;
  useEffect(() => {
    const onKey = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "Escape" || event.isComposing) return;
      // Taken at the window and stopped there: the sheet's dialog listens for Escape on the
      // document and would close with the list.
      event.preventDefault();
      event.stopPropagation();
      dismiss();
    };
    const onPress = (event: PointerEvent) => {
      if (event.target instanceof Node && !anchor.current?.contains(event.target)) dismiss();
    };
    window.addEventListener("keydown", onKey, true);
    document.addEventListener("pointerdown", onPress);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      document.removeEventListener("pointerdown", onPress);
    };
  }, [anchor, dismiss]);
  return rect === null ? null : <PlacedMenu menu={menu} rect={rect} />;
}

/**
 * The session's commands as a listbox beside the composer box. It is drawn inside the composer
 * (never in a portal, so it stays inside the sheet's dialog) at a fixed position, and takes no
 * focus: the text box is the combobox that owns it.
 */
export function CommandMenu({ menu, anchor }: CommandMenuProps) {
  return menu.open ? <OpenMenu menu={menu} anchor={anchor} /> : null;
}
