import { z } from "@/api/zod";

/// The scheduler operation the daemon is wrapped in, as the status snapshot
/// carries it. `what` is the daemon's own description ("verifying the merge of
/// stage-a"); `started_at` is RFC 3339 and constant for one operation, so the
/// page computes elapsed time against its own clock.
export const busySchema = z
  .object({
    op: z.enum(["brief-generation", "merge-verification", "provisioning"]),
    what: z.string(),
    stage_id: z.string(),
    started_at: z.string(),
    ceiling_secs: z.number().int(),
    global_ceiling_secs: z.number().int(),
  })
  .strict();

export type Busy = z.infer<typeof busySchema>;
