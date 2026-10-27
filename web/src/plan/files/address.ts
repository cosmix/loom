export type FileAddress = {
  path: string;
  anchor: string | null;
  diff: "turn" | "open" | "head" | null;
  turn: string | null;
};

export type ResolvedLink =
  | { kind: "doc"; path: string; anchor: string | null }
  | { kind: "anchor"; anchor: string }
  | { kind: "external"; href: string }
  | { kind: "outside"; target: string }
  | { kind: "text" };

export function docRel(path: string): string {
  return path.startsWith("doc/") ? path.slice(4) : path;
}

export function repoRel(path: string): string {
  return `doc/${docRel(path)}`;
}

function decode(value: string): string | null {
  try {
    return decodeURIComponent(value);
  } catch {
    return null;
  }
}

export function toAddress(path: string, anchor?: string | null): string {
  const encodedPath = docRel(path).split("/").map(encodeURIComponent).join("/");
  return `/files/${encodedPath}${anchor ? `#${encodeURIComponent(anchor)}` : ""}`;
}

export function parseAddress(
  location: Pick<Location, "pathname" | "search" | "hash">,
): FileAddress {
  const path = decode(location.pathname.replace(/^\/files\//, "")) ?? "";
  const params = new URLSearchParams(location.search);
  const value = params.get("diff");
  const diff = value === "turn" || value === "open" || value === "head" ? value : null;
  return {
    path: repoRel(path),
    anchor: location.hash ? decode(location.hash.slice(1)) : null,
    diff,
    turn: params.get("turn"),
  };
}

export function resolveLink(fromPath: string, href: string): ResolvedLink {
  const trimmed = href.trim();
  if (trimmed.startsWith("#")) {
    const anchor = decode(trimmed.slice(1));
    return anchor === null ? { kind: "text" } : { kind: "anchor", anchor };
  }
  try {
    const basePath = repoRel(fromPath).split("/").map(encodeURIComponent).join("/");
    const url = new URL(href, `https://doc.invalid/${basePath}`);
    if (
      (url.protocol === "http:" || url.protocol === "https:" || url.protocol === "mailto:") &&
      url.host !== "doc.invalid"
    ) {
      return { kind: "external", href: url.href };
    }
    if (url.protocol !== "https:" || url.host !== "doc.invalid") return { kind: "text" };
    const path = decode(url.pathname);
    const anchor = url.hash ? decode(url.hash.slice(1)) : null;
    if (path === null || (url.hash && anchor === null)) return { kind: "text" };
    // Decode before checking the boundary so encoded separators cannot escape doc/.
    const segments: string[] = [];
    for (const part of path.replaceAll("\\", "/").split("/")) {
      if (part === "..") segments.pop();
      else if (part && part !== ".") segments.push(part);
    }
    if (segments[0] !== "doc" || segments.length < 2) return { kind: "outside", target: path };
    return { kind: "doc", path: segments.join("/"), anchor };
  } catch {
    return { kind: "text" };
  }
}
