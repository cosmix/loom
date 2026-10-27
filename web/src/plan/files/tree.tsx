import { ChevronDownIcon, ChevronRightIcon } from "lucide-react";
import { useEffect, useRef, useState, type JSX } from "react";
import { useNavigate } from "react-router";

import type {
  FileTree,
  SearchHit,
  SearchResults as Results,
  TreeEntry,
} from "@/plan/api/schema-files";
import { toAddress } from "@/plan/files/address";
import { searchDocs } from "@/plan/files/api";
import { SearchBox, type SearchScope } from "@/plan/files/search-box";
import { SearchResults } from "@/plan/files/search-results";
import { TreeEntryRow, containsPath } from "@/plan/files/tree-entry";
import { LoadState } from "@/plan/ui/load-state";

const GROUPS = [
  ["product", "Product"],
  ["plans", "Plans"],
  ["knowledge", "Knowledge"],
  ["other", "Other"],
  ["archive", "Archive"],
] as const satisfies readonly (readonly [keyof FileTree, string])[];

/** Every entry with a document behind it, nested ones included. */
export function treeDocs(tree: FileTree): TreeEntry[] {
  const found: TreeEntry[] = [];
  const visit = (entry: TreeEntry) => {
    if (entry.path !== null) found.push(entry);
    entry.children.forEach(visit);
  };
  GROUPS.forEach(([key]) => tree[key].forEach(visit));
  return found;
}

function matches(entry: TreeEntry, needle: string): boolean {
  return (
    entry.title.toLowerCase().includes(needle) ||
    (entry.file_name ?? "").toLowerCase().includes(needle)
  );
}

/** Entries that match, whole, or that hold a matching descendant (kept with only those). */
function filterEntries(entries: TreeEntry[], needle: string): TreeEntry[] {
  return entries.flatMap((entry) => {
    if (matches(entry, needle)) return [entry];
    const children = filterEntries(entry.children, needle);
    return children.length > 0 ? [{ ...entry, children }] : [];
  });
}

type GroupProps = { label: string; entries: TreeEntry[]; forceOpen: boolean; open: string | null };

function Group({ label, entries, forceOpen, open }: GroupProps): JSX.Element {
  const holdsOpen = entries.some((entry) => containsPath(entry, open));
  const [expanded, setExpanded] = useState(label !== "Archive");
  useEffect(() => {
    if (holdsOpen) setExpanded(true);
  }, [holdsOpen]);
  const shown = expanded || forceOpen;
  const Chevron = shown ? ChevronDownIcon : ChevronRightIcon;
  return (
    <section aria-label={label}>
      <button
        type="button"
        aria-expanded={shown}
        className="flex w-full items-center gap-1 px-2 py-1.5 text-xs font-medium text-muted-foreground"
        onClick={() => setExpanded(!shown)}
      >
        <Chevron className="size-3.5" aria-hidden="true" />
        {label}
      </button>
      {shown && (
        <ul>
          {entries.map((entry) => (
            <TreeEntryRow key={entry.path ?? entry.title} entry={entry} forceOpen={forceOpen} />
          ))}
        </ul>
      )}
    </section>
  );
}

type Search =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; results: Results };

/** The navigation tree under a search box; `open` is the repo path of the open document. */
export function Tree({ tree, open = null }: { tree: FileTree; open?: string | null }): JSX.Element {
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const [scope, setScope] = useState<SearchScope>("all");
  const [search, setSearch] = useState<Search>({ status: "idle" });
  const latest = useRef(0);

  const needle = query.trim().toLowerCase();
  const clear = () => {
    latest.current += 1;
    setQuery("");
    setSearch({ status: "idle" });
  };
  const submit = (searchScope: SearchScope) => {
    const request = ++latest.current;
    setSearch({ status: "loading" });
    void searchDocs(query.trim(), searchScope).then((result) => {
      if (request !== latest.current) return;
      setSearch(
        result.ok
          ? { status: "ready", results: result.value }
          : { status: "error", message: result.message },
      );
    });
  };
  const openHit = (hit: SearchHit) =>
    navigate(toAddress(hit.path, hit.anchor), { state: { marks: hit.terms } });

  return (
    <nav aria-label="Documents" className="flex min-h-0 flex-1 flex-col">
      <SearchBox
        value={query}
        scope={scope}
        onChange={(value) => {
          latest.current += 1;
          setQuery(value);
          setSearch({ status: "idle" });
        }}
        onScope={(next) => {
          setScope(next);
          // Results on screen belong to the old scope; run them again for the new one.
          if (search.status !== "idle") submit(next);
        }}
        onSubmit={() => submit(scope)}
        onClear={clear}
      />
      <div className="min-h-0 flex-1 overflow-y-auto">
        {search.status === "loading" && <LoadState state="loading" title="Searching" />}
        {search.status === "error" && (
          <LoadState state="error" title="Search failed" message={search.message} />
        )}
        {search.status === "ready" && <SearchResults results={search.results} onOpen={openHit} />}
        {search.status === "idle" &&
          GROUPS.map(([key, label]) => {
            const entries = needle ? filterEntries(tree[key], needle) : tree[key];
            if (entries.length === 0) return null;
            return (
              <Group
                key={key}
                label={label}
                entries={entries}
                forceOpen={needle !== ""}
                open={open}
              />
            );
          })}
      </div>
    </nav>
  );
}

/** The slug in a plan file name, whatever its lock prefix: `IN_PROGRESS-PLAN-x.md` gives `x`. */
export function planSlug(fileName: string | null): string | null {
  const found = /^(?:(?:IN_PROGRESS|DONE|RETIRED)-)?PLAN-(.+)\.md$/i.exec(fileName ?? "");
  return found ? found[1].toLowerCase() : null;
}

/** The plan with this slug at a path other than `path`: where a renamed plan went. */
export function renamedPlan(tree: FileTree, slug: string, path: string): TreeEntry | null {
  return (
    treeDocs(tree).find(
      (entry) => entry.kind === "plan" && entry.path !== path && planSlug(entry.file_name) === slug,
    ) ?? null
  );
}
