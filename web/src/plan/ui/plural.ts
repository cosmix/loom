/** "1 plan", "2 plans": the count with its noun in the right number. */
export function plural(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}
