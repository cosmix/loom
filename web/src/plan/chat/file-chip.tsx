import { File, FileImage } from "lucide-react";

import type { FileMeta } from "@/plan/api/schema-chat";
import { previewUrl } from "@/plan/attachments/intake";
import { formatBytes } from "@/plan/ui/format-bytes";
import { ChipRemove } from "./chip-remove";
import { Untrusted, visibleControls } from "./visible-controls";

/**
 * An uploaded file's row: its glyph (or, for an image attached in this tab, a 32px thumbnail from
 * the local file), its name and its size on one line, the remove at the end. Nothing is fetched
 * from the server to draw it. With `onRemove` it is a pending chip; without, a sent one.
 */
export function FileChip({ file, onRemove }: { file: FileMeta; onRemove?: () => void }) {
  const thumbnail = file.media_type.startsWith("image/") ? previewUrl(file.id) : null;
  const Glyph = file.media_type.startsWith("image/") ? FileImage : File;
  const label = visibleControls(file.name);
  return (
    <div className="chat-chip" data-kind="file">
      {thumbnail !== null ? (
        <img src={thumbnail} alt="" className="chat-chip-thumb" />
      ) : (
        <span aria-hidden="true" className="chat-chip-glyph">
          <Glyph />
        </span>
      )}
      <span className="chat-chip-label" title={label}>
        <Untrusted text={file.name} />
      </span>
      <span className="chat-chip-size">{formatBytes(file.bytes)}</span>
      {onRemove && <ChipRemove label={label} onRemove={onRemove} />}
    </div>
  );
}
