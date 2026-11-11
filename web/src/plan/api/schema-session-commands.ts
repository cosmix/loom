import { z } from "@/api/zod";

const u64 = z.number().int().min(0);

export const commandSourceSchema = z.enum([
  "built-in",
  "bundled",
  "plugin",
  "personal",
  "account",
  "skill",
]);
export const commandClassSchema = z.enum(["run", "run-with-handling", "terminal-only"]);
export const catalogueStateSchema = z.enum(["loading", "ready", "unavailable"]);

// One row of the session's command list. `reason` is set on a terminal-only row.
export const commandEntrySchema = z.strictObject({
  name: z.string(),
  aliases: z.array(z.string()),
  description: z.string(),
  argument_hint: z.string(),
  source: commandSourceSchema,
  class: commandClassSchema,
  reason: z.string().nullable(),
});

// The command list of one session: the route body and the fields of the `commands` frame.
// `generation` is null until the list is ready; `reason` is set when it is unavailable.
const commandsViewShape = {
  state: catalogueStateSchema,
  generation: u64.nullable(),
  reason: z.string().nullable(),
  entries: z.array(commandEntrySchema),
};
export const commandsViewSchema = z.strictObject(commandsViewShape);

export const commandsMessageSchemas = [
  z.strictObject({
    type: z.literal("commands"),
    session: z.string(),
    ...commandsViewShape,
  }),
] as const;

export type CommandSource = z.infer<typeof commandSourceSchema>;
export type CommandClass = z.infer<typeof commandClassSchema>;
export type CatalogueState = z.infer<typeof catalogueStateSchema>;
export type CommandEntry = z.infer<typeof commandEntrySchema>;
export type CommandsView = z.infer<typeof commandsViewSchema>;
