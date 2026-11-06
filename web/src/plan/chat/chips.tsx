import { useAtomValue, useStore } from "jotai/react";
import { useEffect, useId, useState, type ReactNode } from "react";
import { Link, useLocation, useNavigate } from "react-router";

import type { Item, Turn } from "@/plan/api/schema-chat";
import type { TreeEntry } from "@/plan/api/schema-files";
import { forgetPreview, uploadStatusAtom } from "@/plan/attachments/intake";
import { pendingAttachmentsAtom, type ChatAttachment } from "@/plan/attachments";
import { openDiff } from "@/plan/diff/open-diff";
import { toAddress } from "@/plan/files/address";
import { treeAtom } from "@/plan/files/atoms";
import { getTurnSections, type TurnSectionsResponse } from "@/plan/review/api";
import { hashesAtom } from "@/plan/review/live";
import { connectionEpochAtom } from "@/plan/state/atoms";
import { OTHER_GLYPHS } from "@/plan/ui/glyphs";
import { chipLabel, chipSource, summaryIdOf, type LabelInput } from "./chip-label";
import { ChipMark } from "./chip-mark";
import { ChipRemove } from "./chip-remove";
import { toAttachmentInput } from "./commands";
import { ErrorLine } from "./error-line";
import { FileChip } from "./file-chip";
import { snapshotAtom } from "./model";
import { usePendingDraft } from "./pending-draft";
import { usePlanFeature } from "./turn-footer-plans";
import { summaryLink } from "./turn-footer-rebuilt";
import { Untrusted, visibleControls } from "./visible-controls";

export { chipLabel, chipSource, summaryIdOf };

const ReadOnlyGlyph = OTHER_GLYPHS.readOnly;

function lockedPath(path: string, entries: TreeEntry[]): boolean {
  return entries.some(
    (entry) => (entry.path === path && entry.locked) || lockedPath(path, entry.children),
  );
}

function useOpenChip() {
  const navigate = useNavigate();
  const location = useLocation();
  const planFeature = usePlanFeature();
  return (a: LabelInput): (() => void) | null => {
    if (a.kind === "summary") {
      const href = summaryLink(summaryIdOf(a.path), planFeature, a.summary_sentence);
      return href === null
        ? null
        : () => {
            void navigate(href);
          };
    }
    return () => {
      void navigate(toAddress(a.path, a.anchor), { state: { returnTo: location.pathname } });
    };
  };
}

/**
 * The hovered chip's full quote. Its source rides in `data-source`, drawn under the quote by
 * chat-chips.css: the row may have no room for the source, and the tooltip's text stays the quote
 * alone (its test reads it whole).
 */
function ChipTooltip({ id, quote, source }: { id: string; quote: string; source: string | null }) {
  return (
    <div
      id={id}
      role="tooltip"
      className="chat-passage"
      data-source={source === null ? undefined : visibleControls(source)}
    >
      <Untrusted text={quote} />
    </div>
  );
}

type ChipProps = {
  attachment: LabelInput;
  quote: string;
  changed: boolean;
  locked: boolean;
  onOpen: (() => void) | null;
  onRemove?: () => void;
  children?: ReactNode;
};

type ChipNameProps = {
  /** The label's words (chip-label.ts) and the same with control characters made visible. */
  text: string;
  label: string;
  source: string | null;
  onOpen: (() => void) | null;
  preview: boolean;
  previewId: string;
  onPreview: (on: boolean) => void;
};

/**
 * The label's words. A quote (`“…”`, chip-label.ts) is split before its closing mark, which gets a
 * non-shrinking element of its own (chat-chips.css), so a quote cut by the row's width ends in
 * `…”` and never loses its mark to the ellipsis.
 */
function ChipLabelText({ label, quoted }: { label: string; quoted: boolean }) {
  if (!quoted) return <Untrusted text={label} />;
  return (
    <>
      <span className="chat-chip-quote">
        <Untrusted text={label.slice(0, -1)} />
      </span>
      <span className="chat-chip-quote-mark">{label.slice(-1)}</span>
    </>
  );
}

/**
 * The row's name and, after it, its source: two items of the row's line (chat-chips.css), the
 * name keeping its width first and the source giving way before it.
 */
function ChipName({ text, label, source, onOpen, preview, previewId, onPreview }: ChipNameProps) {
  // A chip with a source is a selection, and a selection's label is its quote.
  const quoted = source !== null;
  return (
    <>
      {onOpen === null ? (
        <span className="chat-chip-label" title={label} data-quoted={quoted || undefined}>
          <ChipLabelText label={text} quoted={quoted} />
        </span>
      ) : (
        <button
          type="button"
          className="chat-chip-open"
          aria-label={label}
          aria-describedby={preview ? previewId : undefined}
          data-quoted={quoted || undefined}
          onClick={onOpen}
          onMouseEnter={() => onPreview(true)}
          onMouseLeave={() => onPreview(false)}
          onFocus={() => onPreview(true)}
          onBlur={() => onPreview(false)}
        >
          <ChipLabelText label={text} quoted={quoted} />
        </button>
      )}
      {source !== null && (
        <span className="chat-chip-source" title={visibleControls(source)}>
          <Untrusted text={source} />
        </span>
      )}
    </>
  );
}

/**
 * One row: the document's mark, the label (a quoted start, a file name, a place), its source as
 * secondary text on the same line, then what the row knows (locked, changed) and the remove at
 * the end. The label logic is chip-label.ts; this is its presentation only.
 */
function Chip({ attachment, quote, changed, locked, onOpen, onRemove, children }: ChipProps) {
  const text = chipLabel(attachment);
  const label = visibleControls(text);
  const source = chipSource(attachment);
  const [preview, setPreview] = useState(false);
  const previewId = useId();
  return (
    <div className="chat-chip">
      <ChipMark attachment={attachment} />
      <ChipName
        text={text}
        label={label}
        source={source}
        onOpen={onOpen}
        preview={preview}
        previewId={previewId}
        onPreview={setPreview}
      />
      {locked && (
        <span role="img" aria-label="Locked document" className="chat-chip-lock">
          <ReadOnlyGlyph aria-hidden="true" className="size-3" />
        </span>
      )}
      {changed && <span className="chat-chip-note">△ changed</span>}
      {children}
      {onRemove && <ChipRemove label={label} onRemove={onRemove} />}
      {preview && <ChipTooltip id={previewId} quote={quote} source={source} />}
    </div>
  );
}

// Content keys survive removal of an earlier chip; an occurrence counter separates duplicates.
function chipKeys(drafts: ChatAttachment[]): string[] {
  const seen = new Map<string, number>();
  return drafts.map((draft) => {
    // The quote can be large; its length plus the identity fields is enough with the counter.
    const base =
      draft.kind === "file"
        ? JSON.stringify(draft)
        : JSON.stringify({ ...draft, text: draft.text.length });
    const occurrence = seen.get(base) ?? 0;
    seen.set(base, occurrence + 1);
    return `${occurrence}:${base}`;
  });
}

function PendingChip({ draft, onRemove }: { draft: ChatAttachment; onRemove: () => void }) {
  if (draft.kind === "file") return <FileChip file={draft} onRemove={onRemove} />;
  return <PendingDocumentChip draft={draft} onRemove={onRemove} />;
}

function PendingDocumentChip({
  draft,
  onRemove,
}: {
  draft: Exclude<ChatAttachment, { kind: "file" }>;
  onRemove: () => void;
}) {
  const hashes = useAtomValue(hashesAtom);
  const tree = useAtomValue(treeAtom);
  const open = useOpenChip();
  const a = toAttachmentInput(draft);
  const hash = hashes?.docs[a.path];
  const locked =
    a.path.startsWith("doc/loom/knowledge/") ||
    lockedPath(a.path, tree ? Object.values(tree).flat() : []);
  return (
    <Chip
      attachment={a}
      quote={draft.text}
      locked={locked}
      changed={draft.hash !== null && hash !== undefined && draft.hash !== hash}
      onOpen={open(a)}
      onRemove={onRemove}
    />
  );
}

/** Uploads under way and the files that were refused, each reason through Untrusted. */
function UploadNotes() {
  const { busy, problems } = useAtomValue(uploadStatusAtom);
  return (
    <>
      {busy > 0 && (
        <p role="status" className="chat-chip-status">
          {/* Words only: the transcript is where the chat's work shows. */}
          Uploading {busy === 1 ? "a file" : `${busy} files`}…
        </p>
      )}
      {problems.map((problem, index) => (
        <p key={index} role="alert" className="chat-chip-status" data-tone="error">
          <Untrusted text={problem} />
        </p>
      ))}
    </>
  );
}

function MoreButton({
  expanded,
  onClick,
  children,
}: {
  expanded: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      className="chat-link-button chat-chip-more"
      aria-expanded={expanded}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

export function PendingChips() {
  const note = usePendingDraft();
  const pending = useAtomValue(pendingAttachmentsAtom);
  const upload = useAtomValue(uploadStatusAtom);
  const store = useStore();
  const [expanded, setExpanded] = useState(false);
  const count = !expanded && pending.length > 4 ? 3 : pending.length;
  const keys = chipKeys(pending);
  if (!pending.length && !note && upload.busy === 0 && upload.problems.length === 0) return null;
  return (
    <div aria-label="Pending attachments">
      <div className="chat-chip-list">
        {pending.slice(0, count).map((draft, index) => (
          <PendingChip
            key={keys[index]}
            draft={draft}
            onRemove={() => {
              if (draft.kind === "file") forgetPreview(draft.id);
              store.set(pendingAttachmentsAtom, (all) => all.filter((_, at) => at !== index));
            }}
          />
        ))}
        {count < pending.length && (
          <MoreButton expanded={false} onClick={() => setExpanded(true)}>
            +{pending.length - count} more
          </MoreButton>
        )}
      </div>
      {expanded && pending.length > 4 && (
        <MoreButton expanded onClick={() => setExpanded(false)}>
          Show fewer attachments
        </MoreButton>
      )}
      {note && (
        <p role="status" className="chat-chip-status">
          {note}
        </p>
      )}
      <UploadNotes />
    </div>
  );
}

function useTurnSections(turn: Turn) {
  const snapshot = useAtomValue(snapshotAtom);
  const epoch = useAtomValue(connectionEpochAtom);
  const matches = (entry: Turn) => entry.id === turn.id && entry.started_at === turn.started_at;
  // Turn ids and start times can repeat across sessions, so object identity decides first.
  const session = snapshot.turns.includes(turn)
    ? snapshot.session?.id
    : snapshot.previous?.turns.includes(turn)
      ? snapshot.previous.session.id
      : snapshot.turns.some(matches)
        ? snapshot.session?.id
        : snapshot.previous?.turns.some(matches)
          ? snapshot.previous.session.id
          : snapshot.session?.id;
  const [result, setResult] = useState<{
    key: string;
    files?: TurnSectionsResponse["files"];
    error?: string;
  } | null>(null);
  const key = `${session}/${turn.id}/${epoch}/${turn.open}`;
  useEffect(() => {
    if (turn.open || !session) return;
    let live = true;
    void getTurnSections(session, turn.id).then((response) => {
      if (live)
        setResult(
          response.ok ? { key, files: response.value.files } : { key, error: response.message },
        );
    });
    return () => {
      live = false;
    };
  }, [session, turn.id, turn.open, key]);
  return { result: result?.key === key ? result : null, session };
}

export function SentChips({ item, turn }: { item: Extract<Item, { type: "user" }>; turn: Turn }) {
  const { result, session } = useTurnSections(turn);
  const open = useOpenChip();
  if (!item.attachments.length) return null;
  return (
    <div aria-label="Sent attachments" className="chat-chips">
      {item.attachments.map((a, index) => {
        if (a.kind === "file" && a.file) return <FileChip key={index} file={a.file} />;
        const changed = result?.files?.some(
          (file) => file.path === a.path && (a.anchor === null || file.anchors.includes(a.anchor)),
        );
        const href =
          openDiff("turn", a.path, `${session}/${turn.id}`) +
          (a.anchor ? `#${encodeURIComponent(a.anchor)}` : "");
        return (
          <Chip
            key={index}
            attachment={a}
            quote={a.quote}
            changed={a.changed_since_selected}
            locked={a.locked !== null}
            onOpen={open(a)}
          >
            {result?.files &&
              (changed ? (
                <Link to={href} className="chat-chip-note chat-link-button">
                  △ changed in this turn
                </Link>
              ) : (
                <span className="chat-chip-state">unchanged</span>
              ))}
          </Chip>
        );
      })}
      {!turn.open && !result && (
        <p role="status" className="chat-chip-status">
          Checking attached passages…
        </p>
      )}
      {result?.error && <ErrorLine role="status" text={result.error} />}
    </div>
  );
}
