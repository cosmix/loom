import { LockIcon } from "lucide-react";
import { useAtomValue } from "jotai";
import { useId, type ReactElement } from "react";

import type { FetchMode, FetchPolicy } from "@/plan/api/schema-web-fetch";
import { isHolderAtom } from "@/plan/state/atoms";
import { ActionError } from "@/plan/ui/action-error";
import { BusyText } from "@/plan/ui/busy-text";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { AllowlistPanel } from "./allowlist-panel";
import { PaneBlock, SettingsPane } from "./settings-pane";
import { usePolicy, type Save } from "./web-fetch-policy";
import "./web-fetch.css";

/** Each mode with the trade-off it makes; the server applies it from each session's next fetch. */
const MODES: readonly { mode: FetchMode; title: string; sub: string }[] = [
  { mode: "ask", title: "Ask every time", sub: "A decision card for every fetch" },
  {
    mode: "listed",
    title: "Allow listed domains",
    sub: "A listed host and its subdomains go through without asking; any other host asks",
  },
  {
    mode: "always",
    title: "Always allow",
    sub: "Every fetch goes through without asking; the chat lists each one",
  },
  { mode: "never", title: "Never", sub: "Every fetch is refused, even one a chat allowed" },
];
const NOT_HOLDER = "Only the browser holding authoring changes these";

type PolicyProps = { policy: FetchPolicy; editable: boolean; saving: boolean; save: Save };

/** The four modes as one edged list; the chosen row is filled. */
function ModeChoice({ policy, editable, saving, save }: PolicyProps): ReactElement {
  const name = useId();
  return (
    <div role="radiogroup" aria-label="When a session fetches a web page" className="ps-modes">
      {MODES.map(({ mode, title, sub }) => (
        <label key={mode} className="ps-mode">
          <input
            type="radio"
            name={name}
            value={mode}
            checked={policy.mode === mode}
            disabled={!editable || saving}
            aria-labelledby={`${name}-${mode}`}
            aria-describedby={`${name}-${mode}-sub`}
            onChange={() => void save({ ...policy, mode })}
            className="ps-fetch-radio"
          />
          <span className="ps-row-text">
            <span id={`${name}-${mode}`} className="ps-row-title">
              {title}
            </span>
            <span id={`${name}-${mode}-sub`} className="ps-row-sub">
              {sub}
            </span>
          </span>
        </label>
      ))}
    </div>
  );
}

/**
 * Settings → Web fetches: the project's fetch policy, saved on the server. Every browser reads
 * it; only the authoring holder changes it, and the server refuses anyone else.
 */
export function WebFetchPane(): ReactElement {
  const isHolder = useAtomValue(isHolderAtom);
  const { load, error, saving, save, retry } = usePolicy();
  return (
    <SettingsPane
      title="Web fetches"
      scope="project"
      saving={saving}
      lede="An allowed fetch can carry what the session read to the site it names. Every fetch shows in the chat, asked or not."
    >
      {!isHolder && (
        <p className="ps-row-reason" data-kind="info">
          <LockIcon aria-hidden="true" />
          <span>{NOT_HOLDER}</span>
        </p>
      )}
      {load.status === "ready" ? (
        <>
          <PaneBlock title="When a session fetches a web page">
            <ModeChoice policy={load.policy} editable={isHolder} saving={saving} save={save} />
          </PaneBlock>
          {load.policy.mode === "listed" && (
            <div className="ps-block">
              <AllowlistPanel
                policy={load.policy}
                editable={isHolder}
                saving={saving}
                save={save}
              />
            </div>
          )}
        </>
      ) : load.status === "loading" ? (
        <p className="ps-hint ps-block">
          <BusyText>Loading web fetch settings</BusyText>
        </p>
      ) : (
        <div className="ps-block space-y-2 text-[13px]">
          <ActionError message={load.message} />
          <DisabledReason reason={null} label="Retry web fetch settings" onClick={retry}>
            Retry
          </DisabledReason>
        </div>
      )}
      {error && (
        <div className="ps-block">
          <ActionError message={error} />
        </div>
      )}
    </SettingsPane>
  );
}
