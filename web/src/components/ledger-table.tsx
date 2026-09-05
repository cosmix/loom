import { useAtomValue } from "jotai/react";

import { LedgerRow } from "@/components/ledger-row";
import { Skeleton } from "@/components/ui/skeleton";
import { Table, TableBody, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { orderedStagesAtom, snapshotAtom } from "@/state/atoms";

const COLUMNS = [
  "state",
  "stage",
  "depends on",
  "models",
  "activity",
  "context",
  "time",
  "merge",
] as const;

/// The TUI's eight-column ledger. Rows link to the stage page; a state change
/// re-keys the row so it washes in.
export function LedgerTable() {
  const snapshot = useAtomValue(snapshotAtom);
  const ordered = useAtomValue(orderedStagesAtom);

  if (snapshot === null) return <LedgerSkeleton />;
  if (ordered.length === 0) {
    return (
      <p className="rounded-lg border border-dashed border-hairline px-4 py-10 text-center text-sm text-muted-foreground">
        no stages in this workspace
      </p>
    );
  }

  return (
    <div className="ledger rounded-lg border border-hairline bg-card">
      <Table className="text-sm">
        <TableHeader>
          <TableRow className="hover:bg-transparent">
            {COLUMNS.map((column) => (
              <TableHead key={column} scope="col" className="eyebrow h-9 px-3">
                {column}
              </TableHead>
            ))}
          </TableRow>
        </TableHeader>
        <TableBody>
          {ordered.map(({ stage, level }) => (
            <LedgerRow key={`${stage.id}:${stage.status}`} stage={stage} level={level} />
          ))}
        </TableBody>
      </Table>
    </div>
  );
}

const SKELETON_ROWS = [0, 1, 2, 3, 4] as const;

function LedgerSkeleton() {
  return (
    <div
      aria-busy="true"
      aria-label="loading ledger"
      className="flex flex-col gap-3 rounded-lg border border-hairline bg-card p-4"
    >
      <Skeleton className="h-3 w-full" />
      {SKELETON_ROWS.map((row) => (
        <div key={row} className="grid grid-cols-[6rem_1fr_8rem_6rem] gap-4">
          <Skeleton className="h-4" />
          <Skeleton className="h-4" />
          <Skeleton className="h-4" />
          <Skeleton className="h-4" />
        </div>
      ))}
    </div>
  );
}
