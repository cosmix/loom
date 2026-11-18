import type { JSX } from "react";
import { Link } from "react-router";

import type { CodexStatus } from "@/plan/api/schema-pressure";
import { toAddress } from "@/plan/files/address";
import { Untrusted } from "../visible-controls";

function CodexOutcome({ status }: { status: CodexStatus }): JSX.Element | null {
  switch (status.state) {
    case "running":
      return null;
    case "review_written":
      return status.report_path !== null ? (
        <Link className="underline" to={toAddress(status.report_path)}>
          Review written
        </Link>
      ) : (
        <span>Review written</span>
      );
    case "no_review":
      return <span>No review</span>;
    case "failed":
      return <span>Codex failed</span>;
  }
}

/**
 * The detail line under the Codex row's label: the model, and once Codex has finished its outcome
 * (a failed run adds the log tail). The row carries the clock and the turning arrow, so the line
 * holds neither.
 */
export function CodexStatusItem({ status }: { status: CodexStatus }): JSX.Element {
  const running = status.state === "running";
  return (
    <div className="min-w-0 text-xs text-muted-foreground [overflow-wrap:anywhere]">
      <p>
        <Untrusted text={status.model} />
        {!running && (
          <>
            {" · "}
            <CodexOutcome status={status} />
          </>
        )}
      </p>
      {running && (
        <span role="status" aria-live="polite" className="sr-only">
          Codex is reviewing
        </span>
      )}
      {status.state === "failed" && status.log_tail !== null && (
        <pre className="font-mono text-xs break-words whitespace-pre-wrap">
          <code>
            <Untrusted text={status.log_tail} />
          </code>
        </pre>
      )}
    </div>
  );
}
