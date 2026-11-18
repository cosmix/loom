import { useStore } from "jotai/react";
import { useId, type JSX } from "react";

import type { CommandResult, FetchView, Item } from "@/plan/api/schema-chat";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { allowFetchHost, allowSessionFetches } from "./commands";
import { Untrusted } from "./visible-controls";

type Permission = Extract<Item, { type: "permission" }>;
type Send = (command: () => Promise<CommandResult>) => Promise<void>;

/**
 * The transcript line of a fetch a standing rule answered: no card was shown, so the line says
 * where the session reached and which rule let it (or the Settings refusal).
 */
export function fetchOutcome(fetch: FetchView): string | null {
  switch (fetch.rule) {
    case "listed":
      return `Fetch allowed: ${fetch.url} · ${fetch.matched ?? "its host"} is listed in Settings`;
    case "always":
      return `Fetch allowed: ${fetch.url} · Settings allow every fetch`;
    case "session":
      return `Fetch allowed: ${fetch.url} · allowed for this session`;
    case "never":
      return `Fetch refused: ${fetch.url} · Web fetches are turned off in Settings.`;
    case null:
      return null;
  }
}

/**
 * The fetch card's two secondary choices beside Allow and Deny: allow this session's fetches
 * until it ends, or list the host (the server reads it from the request) and allow it from now
 * on. Listing a host in Ask mode moves Settings to Allow listed domains, so the entry applies.
 */
export function FetchChoices({
  item,
  fetch,
  busy,
  send,
}: {
  item: Permission;
  fetch: FetchView;
  busy: boolean;
  send: Send;
}): JSX.Element {
  const store = useStore();
  const explained = useId();
  const host = fetch.listable ? fetch.host : null;
  return (
    <div className="space-y-1">
      <div className="chat-decision-actions">
        <DisabledReason
          reason={null}
          busy={busy}
          shape="pill"
          describedBy={explained}
          onClick={() => void send(() => allowSessionFetches(store, item.request_id))}
        >
          Allow this session
        </DisabledReason>
        {host && (
          <DisabledReason
            reason={null}
            busy={busy}
            shape="pill"
            describedBy={explained}
            onClick={() => void send(() => allowFetchHost(store, item.request_id))}
          >
            Always allow <Untrusted text={host} />
          </DisabledReason>
        )}
      </div>
      <p id={explained} className="plan-fine">
        This session allows every later fetch until the chat ends.
        {host && (
          <>
            {" "}
            Always allow lists <Untrusted text={host} /> and its subdomains in Settings and allows
            listed domains from now on.
          </>
        )}
      </p>
    </div>
  );
}
