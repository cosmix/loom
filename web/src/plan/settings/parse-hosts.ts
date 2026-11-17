/**
 * A pasted list of domains, read for a preview before it is added to the fetch allowlist. The
 * server's `validate_domain` (`loom/src/plan_session/web_fetch.rs`) stays the authority: this
 * check only tells the person what will happen, and a refusal the server still makes shows
 * through the save's error. IDN input passes here and is punycoded by the server.
 */

export type HostStatus = "add" | "listed" | "repeated" | "invalid";

export type HostEntry = {
  /** The piece as pasted. */
  raw: string;
  /** The normalised host; the piece as pasted when `status` is `invalid`. */
  host: string;
  status: HostStatus;
  /** The reason in words, for the row. */
  reason: string;
};

export type HostCounts = Record<HostStatus, number>;

const REASON = {
  add: "Will be added",
  listed: "Already listed",
  repeated: "Repeated in this paste",
} as const;

const MAX_HOST_CHARS = 253;
const MAX_LABEL_CHARS = 63;

/** The host a piece names: no scheme, userinfo, path, port, `*.` or trailing dot. */
export function normaliseHost(piece: string): string {
  return piece
    .trim()
    .toLowerCase()
    .replace(/^[a-z][a-z0-9+.-]*:\/\//, "")
    .replace(/^[^/?#]*@/, "")
    .split(/[/?#]/)[0]!
    .replace(/:\d*$/, "")
    .replace(/^\*\./, "")
    .replace(/\.$/, "");
}

function hasOtherAscii(host: string): boolean {
  for (const char of host) {
    if (char.charCodeAt(0) < 128 && !/[a-z0-9.-]/.test(char)) return true;
  }
  return false;
}

/** Why the server would refuse `host` (already normalised), or null when it would not. */
export function refusalFor(host: string): string | null {
  if (host === "") return "Not a host name";
  const labels = host.split(".");
  // The URL parser reads a name whose last label is a number as an IPv4 address.
  if (/[[\]:]/.test(host) || /^(\d+|0x[0-9a-f]*)$/.test(labels.at(-1)!)) {
    return "An IP address; list a host name";
  }
  if (hasOtherAscii(host)) return "Not a bare host name";
  if (host === "localhost" || host.endsWith(".localhost")) return "Names this machine";
  if (labels.length < 2) return "Needs two labels, as in docs.rs";
  if (host.length > MAX_HOST_CHARS || labels.some((l) => l === "" || l.length > MAX_LABEL_CHARS)) {
    return "Not a valid host name";
  }
  return null;
}

/**
 * Every entry of `text`, in paste order. Commas, semicolons and any whitespace separate entries,
 * in any mix. Each is normalised, then classified, first match winning: invalid, already in
 * `existing`, repeated earlier in the paste, else to be added.
 */
export function parseHosts(text: string, existing: readonly string[]): HostEntry[] {
  const listed = new Set(existing.map((host) => host.toLowerCase()));
  const seen = new Set<string>();
  return text
    .split(/[\s,;]+/)
    .filter((piece) => piece !== "")
    .map((raw): HostEntry => {
      const host = normaliseHost(raw);
      const refusal = refusalFor(host);
      if (refusal !== null) return { raw, host: raw, status: "invalid", reason: refusal };
      if (listed.has(host)) return { raw, host, status: "listed", reason: REASON.listed };
      if (seen.has(host)) return { raw, host, status: "repeated", reason: REASON.repeated };
      seen.add(host);
      return { raw, host, status: "add", reason: REASON.add };
    });
}

export function countHosts(entries: readonly HostEntry[]): HostCounts {
  const counts: HostCounts = { add: 0, listed: 0, repeated: 0, invalid: 0 };
  for (const entry of entries) counts[entry.status] += 1;
  return counts;
}

/** The hosts to save, in paste order. */
export function hostsToAdd(entries: readonly HostEntry[]): string[] {
  return entries.filter((entry) => entry.status === "add").map((entry) => entry.host);
}

/** Whether `text` holds more than one entry: a paste of that shape opens the list review. */
export function isList(text: string): boolean {
  return text.split(/[\s,;]+/).filter((piece) => piece !== "").length > 1;
}
