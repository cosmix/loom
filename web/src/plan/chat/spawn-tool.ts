/** `Task` is the legacy name of `Agent`: both spawn a subagent whose tool calls nest under the row. */
export function isSpawnTool(name: string): boolean {
  return name === "Agent" || name === "Task";
}
