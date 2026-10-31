import { useAtomValue, useStore } from "jotai/react";
import { useEffect, useId, useState, type ReactNode } from "react";
import { Link, useLocation, useNavigate } from "react-router";

import type { Attachment, Item, Turn } from "@/plan/api/schema-chat";
import {
  docKindOfPageKey,
  pageKeyOfPath,
  pageKeyOfSummaryId,
  parsePageKey,
} from "@/plan/api/page-key";
import type { TreeEntry } from "@/plan/api/schema-files";
import { pendingAttachmentsAtom, type AttachmentDraft } from "@/plan/attachments";
import { docColour } from "@/plan/concept/marks";
import { conceptPageAtom } from "@/plan/concept/page-state";
import { openDiff } from "@/plan/diff/open-diff";
import { docRel, toAddress } from "@/plan/files/address";
import { openDocAtom, treeAtom } from "@/plan/files/atoms";
import { treeDocs } from "@/plan/files/tree";
import { getTurnSections, type TurnSectionsResponse } from "@/plan/review/api";
import { hashesAtom } from "@/plan/review/live";
import { connectionEpochAtom } from "@/plan/state/atoms";
import { DocMark } from "@/plan/ui/doc-mark";
import { DOC_KIND_GLYPHS, OTHER_GLYPHS } from "@/plan/ui/glyphs";
import { toAttachmentInput } from "./commands";
import { ErrorLine } from "./error-line";
import { snapshotAtom } from "./model";
import { usePendingDraft } from "./pending-draft";
import { usePlanFeature } from "./turn-footer-plans";
import { summaryLink } from "./turn-footer-rebuilt";
import { Untrusted, visibleControls } from "./visible-controls";

type LabelInput = Pick<
  Attachment,
  "path" | "anchor" | "lines" | "stage" | "summary_sentence" | "kind"
>;

const ReadOnlyGlyph = OTHER_GLYPHS.readOnly;

export function chipLabel(a: LabelInput): string {
  const file = docRel(a.path).split("/").pop() ?? a.path;
  if (a.kind === "summary") {
    const page = parsePageKey(summaryIdOf(a.path));
    if (page?.kind === "overview") {
      return page.scope === "product" ? "Product overview" : "Engineering overview";
    }
    const stem = file.replace(/\.md$/, "");
    return `Summary · ${stem}`;
  }
  const passage = a.stage ? ` › stage ${a.stage}` : a.anchor ? ` › ${a.anchor}` : "";
  const lines = a.lines ? ` · lines ${a.lines[0]}–${a.lines[1]}` : "";
  return `${file}${passage}${lines}`;
}

function lockedPath(path: string, entries: TreeEntry[]): boolean {
  return entries.some(
    (entry) => (entry.path === path && entry.locked) || lockedPath(path, entry.children),
  );
}

/** The summary id a `doc/loom/summaries/<id>.md` path names. */
export function summaryIdOf(path: string): string {
  return docRel(path)
    .replace(/^loom\/summaries\//, "")
    .replace(/\.md$/, "");
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

function ChipMark({ attachment }: { attachment: LabelInput }) {
  const tree = useAtomValue(treeAtom);
  const openDoc = useAtomValue(openDocAtom)?.doc;
  const planFeature = usePlanFeature();
  const location = useLocation();
  const id = attachment.kind === "summary" ? summaryIdOf(attachment.path) : "";
  const key = pageKeyOfSummaryId(id, planFeature);
  const page = useAtomValue(conceptPageAtom(pageKeyOfPath(location.pathname) ?? "")).page;
  const summary = useAtomValue(conceptPageAtom(key ?? "")).page;
  const entry =
    (tree ? treeDocs(tree).find((entry) => entry.path === attachment.path) : null) ??
    (openDoc?.path === attachment.path ? openDoc : null);
  const kind = entry?.kind ?? (key === null ? null : docKindOfPageKey(key));
  const chip = page?.key_chips.find(
    ({ doc }) => doc.path === attachment.path || (key !== null && doc.page === key),
  );
  const colour =
    chip?.colour ?? summary?.colour ?? (page?.path === attachment.path ? page.colour : null);
  if (kind === null || !Object.hasOwn(DOC_KIND_GLYPHS, kind)) return null;
  const docKind = kind as keyof typeof DOC_KIND_GLYPHS;
  return colour == null ? (
    <DocMark kind={docKind} />
  ) : (
    <span
      aria-hidden="true"
      className="inline-flex rounded"
      style={{
        color: docColour(colour),
        backgroundColor: `color-mix(in oklch, ${docColour(colour)} 14%, transparent)`,
      }}
    >
      <DocMark kind={docKind} className="!text-inherit !bg-inherit" />
    </span>
  );
}

function ChipTrailing({
  label,
  locked,
  onRemove,
}: {
  label: string;
  locked: boolean;
  onRemove?: () => void;
}) {
  return (
    <>
      {locked && (
        <span role="img" aria-label="Locked document">
          <ReadOnlyGlyph aria-hidden="true" className="size-3" />
        </span>
      )}
      {onRemove && (
        <button
          type="button"
          className="chat-chip-remove"
          aria-label={`Remove ${label}`}
          onClick={onRemove}
        >
          ×
        </button>
      )}
    </>
  );
}

function ChipTooltip({ id, quote }: { id: string; quote: string }) {
  return (
    <div
      id={id}
      role="tooltip"
      className="chat-passage fixed z-50 max-h-64 max-w-sm overflow-auto whitespace-pre-wrap rounded border bg-popover p-3 shadow-(--dialog-shadow)"
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

function Chip({ attachment, quote, changed, locked, onOpen, onRemove, children }: ChipProps) {
  const label = visibleControls(chipLabel(attachment));
  const [preview, setPreview] = useState(false);
  const previewId = useId();
  return (
    <div className="chat-chip">
      <div className="chat-chip-row">
        <ChipMark attachment={attachment} />
        {onOpen === null ? (
          <span className="truncate">
            <Untrusted text={chipLabel(attachment)} />
          </span>
        ) : (
          <button
            type="button"
            className="chat-chip-open"
            aria-label={label}
            aria-describedby={preview ? previewId : undefined}
            onClick={onOpen}
            onMouseEnter={() => setPreview(true)}
            onMouseLeave={() => setPreview(false)}
            onFocus={() => setPreview(true)}
            onBlur={() => setPreview(false)}
          >
            <Untrusted text={chipLabel(attachment)} />
          </button>
        )}
        <ChipTrailing label={label} locked={locked} onRemove={onRemove} />
      </div>
      {changed && <span className="chat-chip-note">△ changed</span>}
      {children}
      {preview && <ChipTooltip id={previewId} quote={quote} />}
    </div>
  );
}

// Content keys survive removal of an earlier chip; an occurrence counter separates duplicates.
function chipKeys(drafts: AttachmentDraft[]): string[] {
  const seen = new Map<string, number>();
  return drafts.map((draft) => {
    // The quote can be large; its length plus the identity fields is enough with the counter.
    const base = JSON.stringify({ ...draft, text: draft.text.length });
    const occurrence = seen.get(base) ?? 0;
    seen.set(base, occurrence + 1);
    return `${occurrence}:${base}`;
  });
}

function PendingChip({ draft, onRemove }: { draft: AttachmentDraft; onRemove: () => void }) {
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

export function PendingChips() {
  const note = usePendingDraft();
  const pending = useAtomValue(pendingAttachmentsAtom);
  const store = useStore();
  const [expanded, setExpanded] = useState(false);
  const count = !expanded && pending.length > 4 ? 3 : pending.length;
  const keys = chipKeys(pending);
  if (!pending.length && !note) return null;
  return (
    <div aria-label="Pending attachments" className="text-xs">
      <div className="chat-chip-list" data-scroll={expanded || undefined}>
        {pending.slice(0, count).map((draft, index) => (
          <PendingChip
            key={keys[index]}
            draft={draft}
            onRemove={() =>
              store.set(pendingAttachmentsAtom, (all) => all.filter((_, at) => at !== index))
            }
          />
        ))}
        {count < pending.length && (
          <button
            type="button"
            className="chat-link-button plan-fine"
            aria-expanded={false}
            onClick={() => setExpanded(true)}
          >
            +{pending.length - count} more
          </button>
        )}
      </div>
      {expanded && pending.length > 4 && (
        <button
          type="button"
          className="chat-link-button plan-fine"
          aria-expanded={true}
          onClick={() => setExpanded(false)}
        >
          Show fewer attachments
        </button>
      )}
      {note && <p role="status">{note}</p>}
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
                <span>unchanged</span>
              ))}
          </Chip>
        );
      })}
      {!turn.open && !result && (
        <p role="status" className="text-muted-foreground">
          Checking attached passages…
        </p>
      )}
      {result?.error && <ErrorLine role="status" text={result.error} />}
    </div>
  );
}
