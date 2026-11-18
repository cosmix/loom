import { z } from "@/api/zod";
import { apiGet } from "@/plan/api/client";

export const sessionBriefSchema = z.object({
  session_id: z.string(),
  label: z.string(),
  brief: z.string(),
  model: z.string(),
  effort: z.string(),
  display_name: z.string(),
});

export type SessionBriefView = z.infer<typeof sessionBriefSchema>;

export function getSessionBrief(id: string) {
  return apiGet(`/api/session/brief?session=${encodeURIComponent(id)}`, sessionBriefSchema);
}
