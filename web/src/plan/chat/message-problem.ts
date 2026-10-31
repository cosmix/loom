import type { Attachment } from "@/plan/api/schema-chat";
import { formatMessage } from "./format-attachments";
import { CONTROLS } from "./model-identity";

const CAP = 100_000;

/** Why a message cannot be sent as written, or null when it can. The composer and the send gate share it. */
export function messageProblem(text: string, attachments: Attachment[]): string | null {
  const formatted = formatMessage(text, attachments);
  // The pane strips C0/C1 controls after formatting, preserving newline and tab.
  const delivered = formatted.replace(CONTROLS, "");
  const bytes = new TextEncoder().encode(delivered).byteLength;
  const clean = text.replace(CONTROLS, "").replace(/^[\p{White_Space}﻿]+/u, "");
  if (clean.startsWith("/") && clean.split(/\p{White_Space}+/u)[0] !== "/compact") {
    return "Slash commands are not available here; `/compact` is.";
  }
  if (bytes > CAP)
    return `Message with attachments is ${bytes - CAP} byte${bytes - CAP === 1 ? "" : "s"} over the 100,000-byte limit.`;
  if (!delivered.trim()) return "Write a message or attach a passage";
  return null;
}
