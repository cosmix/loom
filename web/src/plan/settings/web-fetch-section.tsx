import { useAtomValue } from "jotai";
import { useEffect, useId, useState, type FormEvent, type ReactElement } from "react";

import {
  getFetchPolicy,
  saveFetchPolicy,
  type FetchMode,
  type FetchPolicy,
} from "@/plan/api/schema-web-fetch";
import { Untrusted } from "@/plan/chat/visible-controls";
import { isHolderAtom } from "@/plan/state/atoms";
import { ActionError } from "@/plan/ui/action-error";
import { BusyText } from "@/plan/ui/busy-text";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { SettingsGroup } from "./settings-sections";
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

type Load =
  | { status: "loading" }
  | { status: "ready"; policy: FetchPolicy }
  | { status: "error"; message: string };

type Save = (policy: FetchPolicy) => Promise<boolean>;

const CHANGED_ELSEWHERE =
  "The web fetch settings changed since they were loaded (another tab, or Always allow on a card). They are reloaded below; check them and try again.";

async function loadPolicy(signal?: AbortSignal): Promise<Load> {
  const result = await getFetchPolicy(signal);
  return result.ok
    ? { status: "ready", policy: result.value }
    : { status: "error", message: result.message };
}

function usePolicy() {
  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    const abort = new AbortController();
    void loadPolicy(abort.signal).then((next) => {
      if (!abort.signal.aborted) setLoad(next);
    });
    return () => abort.abort();
  }, [attempt]);
  const save: Save = async (policy) => {
    setSaving(true);
    setError(null);
    const result = await saveFetchPolicy(policy);
    setSaving(false);
    if (!result.ok) {
      // The policy moved on since it was read: show the server's copy, never overwrite it.
      if (result.status === 409) setLoad(await loadPolicy());
      setError(result.status === 409 ? CHANGED_ELSEWHERE : result.message);
      return false;
    }
    setLoad({ status: "ready", policy: result.value });
    return true;
  };
  const retry = () => {
    setLoad({ status: "loading" });
    setAttempt((count) => count + 1);
  };
  return { load, error, saving, save, retry };
}

function AddDomain({ policy, saving, save }: { policy: FetchPolicy; saving: boolean; save: Save }) {
  const [entry, setEntry] = useState("");
  const inputId = useId();
  const add = async () => {
    if (await save({ ...policy, domains: [...policy.domains, entry.trim()] })) setEntry("");
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (entry.trim() && !saving) void add();
  };
  const reason = saving ? "Saving" : entry.trim() ? null : "Type a host name";
  return (
    <form className="ps-fetch-add" onSubmit={submit}>
      <label htmlFor={inputId} className="ps-row-title">
        Add a domain
      </label>
      <input
        id={inputId}
        value={entry}
        placeholder="docs.rs"
        autoComplete="off"
        spellCheck={false}
        onChange={(event) => setEntry(event.target.value)}
        className="ps-fetch-input"
      />
      <DisabledReason size="sm" reason={reason} onClick={() => void add()}>
        Add
      </DisabledReason>
    </form>
  );
}

function DomainList(props: {
  policy: FetchPolicy;
  editable: boolean;
  saving: boolean;
  save: Save;
}) {
  const { policy, editable, saving, save } = props;
  const remove = (domain: string) =>
    void save({ ...policy, domains: policy.domains.filter((listed) => listed !== domain) });
  return (
    <div className="ps-fetch-list">
      {policy.domains.length === 0 ? (
        <p className="plan-empty">No domains listed yet</p>
      ) : (
        <ul aria-label="Listed domains" className="plan-list">
          {policy.domains.map((domain) => (
            <li key={domain} className="ps-fetch-domain">
              <span className="ps-fetch-host">
                <Untrusted text={domain} />
              </span>
              {editable && (
                <DisabledReason
                  size="sm"
                  look="quiet"
                  reason={saving ? "Saving" : null}
                  label={`Remove ${domain}`}
                  onClick={() => remove(domain)}
                >
                  Remove
                </DisabledReason>
              )}
            </li>
          ))}
        </ul>
      )}
      {editable && <AddDomain policy={policy} saving={saving} save={save} />}
    </div>
  );
}

function ModeChoice(props: {
  policy: FetchPolicy;
  editable: boolean;
  saving: boolean;
  save: Save;
}) {
  const { policy, editable, saving, save } = props;
  const name = useId();
  return (
    <div role="radiogroup" aria-label="When a session fetches a web page">
      {MODES.map(({ mode, title, sub }) => (
        <div key={mode} className="ps-row-wrap">
          <label className="ps-row">
            <span className="ps-row-text">
              <span id={`${name}-${mode}`} className="ps-row-title">
                {title}
              </span>
              <span id={`${name}-${mode}-sub`} className="ps-row-sub">
                {sub}
              </span>
            </span>
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
          </label>
          {mode === "listed" && policy.mode === "listed" && <DomainList {...props} />}
        </div>
      ))}
    </div>
  );
}

/**
 * Settings → Web fetches: the project's fetch policy, saved on the server. Every browser reads
 * it; only the authoring holder changes it, and the server refuses anyone else.
 */
export function WebFetchGroup(): ReactElement {
  const isHolder = useAtomValue(isHolderAtom);
  const { load, error, saving, save, retry } = usePolicy();
  return (
    <SettingsGroup title="Web fetches" hint="Saved for this project">
      <p className="ps-fetch-note">
        An allowed fetch can carry what the session read to the site it names. Every fetch shows in
        the chat, asked or not.
      </p>
      {!isHolder && <p className="ps-status">{NOT_HOLDER}</p>}
      {load.status === "ready" ? (
        <ModeChoice policy={load.policy} editable={isHolder} saving={saving} save={save} />
      ) : load.status === "loading" ? (
        <p className="ps-hint">
          <BusyText>Loading web fetch settings</BusyText>
        </p>
      ) : (
        <div className="space-y-2 text-[13px]">
          <ActionError message={load.message} />
          <DisabledReason reason={null} label="Retry web fetch settings" onClick={retry}>
            Retry
          </DisabledReason>
        </div>
      )}
      {saving && (
        <p className="ps-status">
          <BusyText>Saving</BusyText>
        </p>
      )}
      {error && <ActionError message={error} />}
    </SettingsGroup>
  );
}
