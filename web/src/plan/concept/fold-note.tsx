import type { JSX } from "react";
import { Link } from "react-router";

import { toAddress } from "@/plan/files/address";

import type { FoldNote } from "./types";

/**
 * The muted line that takes a folded run's place when its sections or sentences are gone.
 * It stays until the reader opens or folds something in the same paragraph.
 */
export function FoldNoteLine({ note }: { note: FoldNote }): JSX.Element {
  return (
    <div role="note" className="concept-note">
      {note.text}.{" "}
      <Link to={toAddress(note.path)}>
        Open <span className="font-mono">{note.path.slice(note.path.lastIndexOf("/") + 1)}</span> in
        file mode
      </Link>
    </div>
  );
}
