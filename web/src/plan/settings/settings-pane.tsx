import { FolderIcon, MonitorIcon } from "lucide-react";
import type { ReactElement, ReactNode } from "react";

import { BusyText } from "@/plan/ui/busy-text";

/** Where a setting is kept: this browser's storage, or the project on the server. */
export type Scope = "browser" | "project";

const SCOPE = {
  browser: { Icon: MonitorIcon, words: "Remembered in this browser" },
  project: { Icon: FolderIcon, words: "Saved for this project" },
} as const;

/** One glyph and the scope's words; while a save runs the words give way to "Saving". */
function ScopeLabel({ scope, saving }: { scope: Scope; saving: boolean }): ReactElement {
  const { Icon, words } = SCOPE[scope];
  return (
    <span className="ps-scope">
      {saving ? (
        <BusyText>Saving</BusyText>
      ) : (
        <>
          <Icon aria-hidden="true" />
          {words}
        </>
      )}
    </span>
  );
}

/**
 * What one rail tab shows: the page title with its scope at the head's end, an optional lede,
 * then the pane's blocks. The tab panel around it carries the tab's name, so this draws no
 * landmark of its own.
 */
export function SettingsPane({
  title,
  scope,
  hint,
  saving = false,
  lede,
  children,
}: {
  title: string;
  /** Omit for a pane whose head carries a `hint` instead. */
  scope?: Scope;
  hint?: string;
  saving?: boolean;
  lede?: string;
  children: ReactNode;
}): ReactElement {
  return (
    <>
      <div className="ps-pane-head">
        <h2 className="ps-pane-title">{title}</h2>
        {scope ? (
          <ScopeLabel scope={scope} saving={saving} />
        ) : (
          <span className="ps-hint">{hint}</span>
        )}
      </div>
      {lede && <p className="ps-lede">{lede}</p>}
      {children}
    </>
  );
}

/** A titled part of a pane: its own h3 under the pane's h2, with an optional one-line hint. */
export function PaneBlock({
  title,
  hint,
  children,
}: {
  title: string;
  hint?: string;
  children: ReactNode;
}): ReactElement {
  return (
    <div className="ps-block">
      <div className="ps-block-head">
        <h3 className="ps-block-title">{title}</h3>
        {hint && <span className="ps-hint">{hint}</span>}
      </div>
      {children}
    </div>
  );
}
