import { toAddress } from "@/plan/files/address";

export function openDiff(kind: "turn" | "open" | "head", path: string, turn?: string): string {
  const turnQuery = turn === undefined ? "" : `&turn=${encodeURIComponent(turn)}`;
  return `${toAddress(path)}?diff=${kind}${turnQuery}`;
}
