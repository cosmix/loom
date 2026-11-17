import { ClipboardPasteIcon, XIcon } from "lucide-react";
import {
  useEffect,
  useId,
  useRef,
  useState,
  type ClipboardEvent,
  type FormEvent,
  type ReactElement,
} from "react";

import type { FetchPolicy } from "@/plan/api/schema-web-fetch";
import { Untrusted } from "@/plan/chat/visible-controls";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { isList } from "./parse-hosts";
import { PastePanel } from "./paste-review";
import type { Save } from "./web-fetch-policy";

type PanelProps = { policy: FetchPolicy; editable: boolean; saving: boolean; save: Save };

/**
 * One entry typed into the field, saved as typed: the server lowercases, validates and
 * refuses. A paste of several entries opens the list review instead (`onList`).
 */
function AddDomain({
  policy,
  saving,
  save,
  onList,
}: Omit<PanelProps, "editable"> & { onList: (text: string) => void }): ReactElement {
  const [entry, setEntry] = useState("");
  const inputId = useId();
  const hintId = useId();
  const add = async () => {
    if (await save({ ...policy, domains: [...policy.domains, entry.trim()] })) setEntry("");
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (entry.trim() && !saving) void add();
  };
  const paste = (event: ClipboardEvent<HTMLInputElement>) => {
    const text = event.clipboardData.getData("text");
    if (!isList(text)) return;
    event.preventDefault();
    onList(text);
  };
  const reason = saving ? "Saving" : entry.trim() ? null : "Type a host name";
  return (
    <form className="ps-fetch-add" onSubmit={submit}>
      <div className="ps-fetch-add-row">
        <input
          id={inputId}
          value={entry}
          placeholder="docs.rs"
          aria-label="Add a domain"
          autoComplete="off"
          spellCheck={false}
          onChange={(event) => setEntry(event.target.value)}
          onPaste={paste}
          className="ps-fetch-input"
        />
        <DisabledReason size="sm" reason={reason} reasonId={hintId} onClick={() => void add()}>
          Add
        </DisabledReason>
      </div>
      <p id={hintId} className="ps-hint">
        Host names only, as in docs.rs. Paste several at once, separated by commas or new lines.
      </p>
    </form>
  );
}

function DomainList({ policy, editable, saving, save }: PanelProps): ReactElement {
  const remove = (domain: string) =>
    void save({ ...policy, domains: policy.domains.filter((listed) => listed !== domain) });
  if (policy.domains.length === 0) {
    return <p className="plan-empty ps-panel-empty">No domains listed yet</p>;
  }
  return (
    <ul aria-label="Listed domains" className="ps-hosts">
      {policy.domains.map((domain) => (
        <li key={domain}>
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
              <XIcon aria-hidden="true" />
            </DisabledReason>
          )}
        </li>
      ))}
    </ul>
  );
}

/**
 * The allowlist as its own edged panel: a head with the count and Paste a list, the add row,
 * then the hosts. Paste a list, or a multi-entry paste into the field, swaps the panel for the
 * text area beside its review; leaving it puts focus back on the button that opened it.
 */
export function AllowlistPanel(props: PanelProps): ReactElement {
  const { policy, editable, saving, save } = props;
  const [pasted, setPasted] = useState<string | null>(null);
  const panel = useRef<HTMLDivElement>(null);
  const wasPasting = useRef(false);
  useEffect(() => {
    if (pasted === null && wasPasting.current) {
      panel.current?.querySelector<HTMLElement>(".ps-panel-tool button")?.focus();
    }
    wasPasting.current = pasted !== null;
  }, [pasted]);
  if (pasted !== null) {
    return (
      <PastePanel
        policy={policy}
        saving={saving}
        save={save}
        initial={pasted}
        onClose={() => setPasted(null)}
      />
    );
  }
  return (
    <div ref={panel} className="ps-panel">
      <div className="ps-panel-head">
        <h3 className="ps-panel-title">Listed domains</h3>
        <span className="ps-hint">{policy.domains.length}</span>
        {editable && (
          <div className="ps-panel-tool">
            <DisabledReason size="sm" reason={null} onClick={() => setPasted("")}>
              <ClipboardPasteIcon aria-hidden="true" />
              Paste a list
            </DisabledReason>
          </div>
        )}
      </div>
      {editable && <AddDomain policy={policy} saving={saving} save={save} onList={setPasted} />}
      <DomainList {...props} />
    </div>
  );
}
