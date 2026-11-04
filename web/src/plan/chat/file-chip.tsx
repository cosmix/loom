import { File, FileImage } from "lucide-react";

import type { FileMeta } from "@/plan/api/schema-chat";
import { previewUrl } from "@/plan/attachments/intake";
import { formatBytes } from "@/plan/ui/format-bytes";
import { Untrusted, visibleControls } from "./visible-controls";

/**
 * An uploaded file's chip: its name and size, and for an image attached in this tab a thumbnail
 * from the local file. Nothing is fetched from the server to draw it. With `onRemove` it is a
 * pending chip; without, a sent one.
 */
export function FileChip({ file, onRemove }: { file: FileMeta; onRemove?: () => void }) {
  const thumbnail = file.media_type.startsWith("image/") ? previewUrl(file.id) : null;
  const Glyph = file.media_type.startsWith("image/") ? FileImage : File;
  const label = visibleControls(file.name);
  return (
    <div className="chat-chip" data-kind="file">
      <div className="chat-chip-row">
        {thumbnail !== null ? (
          <img src={thumbnail} alt="" className="chat-chip-thumb" />
        ) : (
          <Glyph aria-hidden="true" className="size-4 shrink-0" />
        )}
        <span className="truncate">
          <Untrusted text={file.name} />
        </span>
        <span className="chat-chip-size plan-fine">{formatBytes(file.bytes)}</span>
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
      </div>
    </div>
  );
}
