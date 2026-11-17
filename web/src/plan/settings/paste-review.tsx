import {
  ArrowLeftIcon,
  CircleAlertIcon,
  CopyIcon,
  EqualIcon,
  PlusIcon,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useId, useMemo, useRef, useState, type ReactElement } from "react";

import type { FetchPolicy } from "@/plan/api/schema-web-fetch";
import { Untrusted } from "@/plan/chat/visible-controls";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import {
  countHosts,
  hostsToAdd,
  parseHosts,
  type HostCounts,
  type HostEntry,
  type HostStatus,
} from "./parse-hosts";
import type { Save } from "./web-fetch-policy";

const GLYPH: Record<HostStatus, LucideIcon> = {
  add: PlusIcon,
  listed: EqualIcon,
  repeated: CopyIcon,
  invalid: CircleAlertIcon,
};

function ReviewRow({ entry }: { entry: HostEntry }): ReactElement {
  const Glyph = GLYPH[entry.status];
  return (
    <li data-status={entry.status}>
      <Glyph aria-hidden="true" className="ps-review-glyph" />
      <span className="ps-fetch-host">
        <Untrusted text={entry.host} />
      </span>
      <span className="ps-review-word">{entry.reason}</span>
      {entry.status !== "invalid" && entry.host !== entry.raw && (
        <span className="ps-review-from">
          from <Untrusted text={entry.raw} />
        </span>
      )}
    </li>
  );
}

function summary(counts: HostCounts, total: number): ReactElement {
  if (total === 0) return <>Nothing pasted yet</>;
  const rest = [
    counts.listed > 0 && `${counts.listed} already listed`,
    counts.repeated > 0 && `${counts.repeated} repeated`,
    counts.invalid > 0 && `${counts.invalid} can’t be listed`,
  ].filter(Boolean);
  return (
    <>
      <b>{counts.add} to add</b>
      {rest.map((part) => ` · ${part}`).join("")}
    </>
  );
}

/** The commit button's words: the count it adds, or why it adds nothing. */
function commitWords(count: number): string {
  return count === 0 ? "Add domains" : `Add ${count} ${count === 1 ? "domain" : "domains"}`;
}

/**
 * The allowlist panel in paste mode: a text area beside a live review of what each entry would
 * do, and in the foot the counts, Cancel and the one save. The review is read against the
 * policy as loaded, so a 409 that reloads the policy re-runs it against the server's list.
 */
export function PastePanel({
  policy,
  saving,
  save,
  initial,
  onClose,
}: {
  policy: FetchPolicy;
  saving: boolean;
  save: Save;
  initial: string;
  onClose: () => void;
}): ReactElement {
  const [text, setText] = useState(initial);
  const area = useRef<HTMLTextAreaElement>(null);
  const ids = { area: useId(), hint: useId(), review: useId(), sum: useId() };
  const entries = useMemo(() => parseHosts(text, policy.domains), [text, policy.domains]);
  const toAdd = hostsToAdd(entries);
  useEffect(() => {
    const field = area.current;
    if (!field) return;
    field.focus();
    field.setSelectionRange(field.value.length, field.value.length);
  }, []);
  const add = async () => {
    if (await save({ ...policy, domains: [...policy.domains, ...toAdd] })) onClose();
  };
  const reason = saving ? "Saving" : toAdd.length === 0 ? "Nothing to add" : null;
  return (
    <div className="ps-panel">
      <div className="ps-panel-head">
        <h3 className="ps-panel-title">Paste a list</h3>
        <span className="ps-hint">{policy.domains.length} listed now</span>
        <div className="ps-panel-tool">
          <DisabledReason size="sm" look="quiet" reason={null} onClick={onClose}>
            <ArrowLeftIcon aria-hidden="true" />
            Back to the list
          </DisabledReason>
        </div>
      </div>
      <div className="ps-split">
        <div>
          <label htmlFor={ids.area} className="ps-split-label">
            Pasted text
          </label>
          <textarea
            ref={area}
            id={ids.area}
            value={text}
            rows={9}
            spellCheck={false}
            autoComplete="off"
            aria-describedby={ids.hint}
            onChange={(event) => setText(event.target.value)}
            className="ps-fetch-input ps-fetch-area"
          />
          <span id={ids.hint} className="ps-hint ps-split-hint">
            Commas, spaces and new lines all separate entries. A URL is cut down to its host.
          </span>
        </div>
        <div>
          <h4 id={ids.review} className="ps-split-label">
            What will happen
          </h4>
          <ul aria-labelledby={ids.review} className="ps-review">
            {entries.map((entry, index) => (
              <ReviewRow key={`${index}:${entry.raw}`} entry={entry} />
            ))}
          </ul>
        </div>
      </div>
      <div className="ps-panel-foot">
        <p id={ids.sum} role="status" className="ps-review-sum">
          {summary(countHosts(entries), entries.length)}
        </p>
        <div className="ps-panel-actions">
          <DisabledReason size="sm" look="quiet" reason={null} onClick={onClose}>
            Cancel
          </DisabledReason>
          <DisabledReason
            look="primary"
            reason={reason}
            reasonId={ids.sum}
            onClick={() => void add()}
          >
            {commitWords(toAdd.length)}
          </DisabledReason>
        </div>
      </div>
    </div>
  );
}
