import { useEffect, useState } from "react";

import type { TraceView } from "@/plan/api/schema-trace";
import { fetchDoc } from "@/plan/files/api";

import { ledgerPath } from "./findings-actions";

// The findings ledger's save hash as the page last read it. The status route refuses a hash that
// no longer matches the file (409), so the hash has to belong to the trace the cards were drawn
// from: it is fetched when a trace loads, not when a button is pressed.

export interface LedgerHash {
  /** The ledger document's hash; null while it is being read or when it could not be read. */
  hash: string | null;
  /** Why the ledger could not be read; null while reading and once read. */
  problem: string | null;
}

const READING: LedgerHash = { hash: null, problem: null };

/** The words that block a change until the page holds the ledger's hash; null when it does. */
export function ledgerBlock(ledger: LedgerHash): string | null {
  if (ledger.hash !== null) return null;
  return ledger.problem ?? "Reading the ledger";
}

/**
 * The ledger hash that goes with `trace`. Each new trace object restarts the read, and a hash
 * read for an earlier trace is never returned for a later one.
 */
export function useLedgerHash(feature: string, trace: TraceView): LedgerHash {
  const [held, setHeld] = useState<{ trace: TraceView; value: LedgerHash } | null>(null);
  const hasFindings = trace.findings.length > 0;
  useEffect(() => {
    // Without findings there is nothing to change, and a missing ledger would log a 404 each visit.
    if (!hasFindings) return;
    let current = true;
    void fetchDoc(ledgerPath(feature)).then((result) => {
      if (!current) return;
      const value = result.ok
        ? { hash: result.value.hash, problem: null }
        : { hash: null, problem: result.message };
      setHeld({ trace, value });
    });
    return () => {
      current = false;
    };
  }, [feature, trace, hasFindings]);
  return held !== null && held.trace === trace ? held.value : READING;
}
