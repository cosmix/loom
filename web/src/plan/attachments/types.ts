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
