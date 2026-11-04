export type AttachmentDraft =
  | {
      kind: "source";
      path: string;
      anchor: string | null;
      text: string;
      lines: { start: number; end: number } | null;
      stageId: string | null;
      hash: string | null;
    }
  | {
      kind: "summary";
      summary: string;
      sentence: string;
      text: string;
      restsOn: { path: string; anchor: string }[];
      hash: string | null;
    };

/**
 * An uploaded file in the chat's chip list. It is separate from `AttachmentDraft` because that type
 * is read for `text`, `path`, `anchor` and `lines` by the reading and review screens. Only the
 * metadata is kept; the bytes live on the server and the chip travels by `id`.
 */
export type FileDraft = {
  kind: "file";
  id: string;
  name: string;
  media_type: string;
  bytes: number;
};

/** What the chat's chip list and saved draft hold. */
export type ChatAttachment = AttachmentDraft | FileDraft;
