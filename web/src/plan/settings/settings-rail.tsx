import type { LucideIcon } from "lucide-react";
import { type KeyboardEvent, type ReactElement } from "react";

export type RailPane<Id extends string> = { id: Id; label: string; Icon: LucideIcon };

const STEP: Record<string, 1 | -1> = {
  ArrowDown: 1,
  ArrowRight: 1,
  ArrowUp: -1,
  ArrowLeft: -1,
};

/** The index a key moves to from `index` among `count` tabs, wrapping; null for any other key. */
function targetIndex(key: string, index: number, count: number): number | null {
  if (key === "Home") return 0;
  if (key === "End") return count - 1;
  const step = STEP[key];
  return step === undefined ? null : (index + step + count) % count;
}

export const tabId = (base: string, pane: string) => `${base}-tab-${pane}`;
export const panelId = (base: string) => `${base}-panel`;

/**
 * The rail: one tab per pane. A tab is chosen on focus (arrow keys, Home and End, wrapping), so
 * the chosen pane always follows the focused tab, and only that tab is in the Tab order. Left and
 * Right move as well as Up and Down.
 */
export function SettingsRail<Id extends string>({
  panes,
  current,
  onSelect,
  idBase,
}: {
  panes: readonly RailPane<Id>[];
  current: Id;
  onSelect: (id: Id) => void;
  idBase: string;
}): ReactElement {
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const tabs = Array.from(event.currentTarget.querySelectorAll<HTMLElement>('[role="tab"]'));
    const from = tabs.indexOf(document.activeElement as HTMLElement);
    const to = from < 0 ? null : targetIndex(event.key, from, tabs.length);
    if (to === null) return;
    event.preventDefault();
    tabs[to]!.focus();
    onSelect(panes[to]!.id);
  };
  return (
    <div
      role="tablist"
      aria-label="Settings sections"
      aria-orientation="vertical"
      className="ps-rail"
      onKeyDown={onKeyDown}
    >
      {panes.map(({ id, label, Icon }) => (
        <button
          key={id}
          type="button"
          role="tab"
          id={tabId(idBase, id)}
          aria-selected={id === current}
          aria-controls={panelId(idBase)}
          tabIndex={id === current ? 0 : -1}
          className="ps-tab"
          onClick={() => onSelect(id)}
        >
          <Icon aria-hidden="true" />
          {label}
        </button>
      ))}
    </div>
  );
}
