import { FileDiff } from "lucide-react";
import { memo, useMemo, type JSX } from "react";
import { Link } from "react-router";

import type { Turn } from "@/plan/api/schema-chat";
import type { DiffTexts } from "@/plan/api/schema-files";
import { openDiff } from "@/plan/diff/open-diff";
import { docRel } from "@/plan/files/address";
import { ImpactRows } from "@/plan/trace/impact-rows";
import { plural } from "@/plan/ui/plural";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import type { DocKind } from "@/plan/ui/glyphs";
import { TurnFooterExtras } from "./footer-extras";
import { lineCounts } from "./line-diff";
import { RestoreAll, RestoreButton } from "./restore";
import {
  ImplementingPlans,
  sectionFile,
  sectionId,
  useImplementedBy,
  type ImplementedBy,
} from "./turn-footer-plans";
import { RebuiltSummaries } from "./turn-footer-rebuilt";
import { useTurnTexts, type TurnRow } from "./use-turn-texts";
import { Untrusted, visibleControls } from "./visible-controls";

type File = Turn["changed_files"][number];
const displayStyle = { unicodeBidi: "plaintext", direction: "ltr" } as const;

function changedDocKind(path: string): DocKind | null {
  if (sectionFile.test(path)) return "section";
  if (/^doc\/product\/(?:[a-z0-9-]+\/)?index\.md$/.test(path)) return "vision";
  if (path.startsWith("doc/loom/knowledge/")) return "knowledge";
  if (/^doc\/plans\/(?:[a-z0-9-]+\/)?briefs\/.+/.test(path)) return "brief";
  const name = path.match(/^doc\/plans\/(?:[a-z0-9-]+\/)?(?:archive\/)?([^/]+)$/)?.[1];
  if (name && /^(?:(?:IN_PROGRESS|DONE|RETIRED)-)?PLAN-.+\.md$/.test(name)) return "plan";
  if (/^doc\/plans\/(?:[a-z0-9-]+\/)?index\.md$/.test(path)) return "metaplan";
  return null;
}

function changeLabel(file: File, texts?: DiffTexts): string {
  if (file.before_hash === null || texts?.before === null) return "new";
  if (!texts) return "";
  if (texts.after === null) return "deleted";
  const { added, removed } = lineCounts(texts.before, texts.after);
  return `+${added} −${removed}`;
}

type RowProps = { file: File; row?: TurnRow; rows: TurnRow[]; turn: Turn; sessionId: string };

function RowActions({ file, row, rows, turn, sessionId }: RowProps) {
  if (turn.open)
    return (
      <DisabledReason
        reason="Available when the turn ends"
        onClick={() => {}}
        size="sm"
        look="quiet"
      >
        Diff
      </DisabledReason>
    );
  // A link drawn at the quiet row button's size, beside Restore (restore.tsx, a `sm` button).
  return (
    <>
      <Link
        className="plan-button chat-footer-action"
        data-look="quiet"
        data-size="sm"
        to={openDiff("turn", docRel(file.path), `${sessionId}/${turn.id}`)}
      >
        Diff
      </Link>
      {file.before_hash !== null && row?.texts?.after !== null && (
        <RestoreButton sessionId={sessionId} turn={turn} path={file.path} texts={rows} />
      )}
    </>
  );
}

function ChangedRow(props: RowProps) {
  const { file, row } = props;
  const inside = file.path.startsWith("doc/");
  const kind = changedDocKind(file.path);
  // Line counting walks both whole documents: redo it only when the fetched texts change.
  const label = useMemo(() => changeLabel(file, row?.texts), [file, row?.texts]);
  return (
    <div className="chat-changed-row">
      <span
        className="chat-file-chip"
        data-doc-kind={kind ?? undefined}
        title={visibleControls(docRel(file.path))}
      >
        <FileDiff size={12} aria-hidden="true" />
        <Untrusted text={file.path.split("/").pop() ?? file.path} />
        {label && <span className="chat-file-counts">{label}</span>}
      </span>
      {inside ? (
        <RowActions {...props} />
      ) : (
        <span>
          <span aria-hidden="true" style={{ color: "var(--tone-warning)" }}>
            ⚠
          </span>{" "}
          outside doc/
        </span>
      )}
      {row?.error && (
        <span role="alert">
          <Untrusted text={row.error} />
        </span>
      )}
    </div>
  );
}

/** The slugs of the plans `ImplementingPlans` draws under a changed section file. */
function shownSlugs(path: string, implemented: ImplementedBy): ReadonlySet<string> {
  const id = sectionId(path);
  if (id === null || !implemented.sections) return new Set();
  return new Set((implemented.sections[id] ?? []).map((plan) => plan.slug));
}

function ChangedFiles({
  turn,
  sessionId,
  rows,
}: {
  turn: Turn;
  sessionId: string;
  rows: TurnRow[];
}) {
  const byPath = useMemo(() => new Map(rows.map((row) => [row.path, row])), [rows]);
  // One request for the whole turn, however many section files changed.
  const implemented = useImplementedBy(
    turn.changed_files.some((file) => sectionFile.test(file.path)),
  );
  if (turn.changed_files.length === 0) return null;
  // The files are quiet chips on one wrapping row under the turn, each with its actions.
  return (
    <details className="chat-changed">
      <summary>
        {turn.open ? "Changing" : "Changed"} {plural(turn.changed_files.length, "file")}
      </summary>
      <ul className="chat-changed-files">
        {turn.changed_files.map((file) => (
          <li key={file.path}>
            <ChangedRow
              file={file}
              row={byPath.get(file.path)}
              rows={rows}
              turn={turn}
              sessionId={sessionId}
            />
            {sectionFile.test(file.path) && (
              <ImplementingPlans path={file.path} loaded={implemented} />
            )}
            <ImpactRows path={file.path} omitPlans={shownSlugs(file.path, implemented)} />
          </li>
        ))}
      </ul>
    </details>
  );
}

function modifiedPaths(turn: Turn, rows: TurnRow[]): string[] {
  const byPath = new Map(rows.map((row) => [row.path, row.texts]));
  return turn.changed_files
    .filter((file) => {
      const texts = byPath.get(file.path);
      return (
        file.path.startsWith("doc/") &&
        file.before_hash !== null &&
        texts?.before !== null &&
        texts?.after !== null
      );
    })
    .map((file) => file.path);
}

function TurnFooterView({ turn, sessionId }: { turn: Turn; sessionId: string }): JSX.Element {
  const rows = useTurnTexts(turn, sessionId);
  const modified = useMemo(() => modifiedPaths(turn, rows), [turn, rows]);
  const number = turn.id.replace(/^t/, "");
  return (
    <div
      role="group"
      aria-label={`Turn ${visibleControls(number)} changes`}
      className="chat-turn-footer"
      style={displayStyle}
    >
      <ChangedFiles turn={turn} sessionId={sessionId} rows={rows} />
      {modified.length >= 2 && (
        <RestoreAll sessionId={sessionId} turn={turn} paths={modified} texts={rows} />
      )}
      <RebuiltSummaries turn={turn} />
      <TurnFooterExtras turn={turn} />
    </div>
  );
}

/** Memoised so a chat delta re-renders only the footers whose turn object changed. */
export const TurnFooter = memo(TurnFooterView);
