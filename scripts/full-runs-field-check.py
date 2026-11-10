"""Find likely FULL test-suite runs by stage main agents and their subagents.

TEMPORARY: this script exists only for the full-suite-at-gate field check and
is deleted after it (plan full-suite-at-gate, Acceptance item 4).
"""
import argparse, datetime, json, re, time
from pathlib import Path
from collections import Counter

# A runner invocation followed only by flags / redirects / pipes => full run.
RUNNERS = [
    r"cargo (?:\+\S+ )?test", r"cargo nextest run", r"(?:bun|npm|pnpm|yarn)(?: run)? test",
    r"(?:bunx |npx |node_modules/\.bin/)?vitest(?: run)?", r"(?:uv run |python3? -m )?pytest",
    r"go test(?: \./\.\.\.)?", r"make (?:test|check)", r"just test", r"mvn test", r"gradlew? test",
    r"dotnet test", r"mix test", r"rspec", r"(?:bundle exec )?rake test", r"phpunit", r"jest",
]
FULL = re.compile(
    r"(?:^|[;&|(]\s*|\bcd \S+ && |\btimeout \d+ |\benv \S+ )(" + "|".join(RUNNERS) + r")"
    r"((?:\s+(?:-{1,2}[\w-]+(?:=\S+)?|\d?>&?\d?|>\s*\S+|2>&1|--manifest-path \S+|--no-fail-fast|--all-targets|--workspace|--all|-q|--quiet))*)"
    r"\s*(?:$|[;&|)])"
)
SCOPED_FLAGS = re.compile(r"--test\b|--lib\b|--bin\b|-p\b|--package|-k\b|-t\b|-run\b|--filter|--testNamePattern")
BLOCKED_MARK = "[subagent-verify-guard/"
# Claude Code names a project directory after its absolute path with every
# non-alphanumeric character replaced by "-"; this is the prefix of the
# directory holding the main checkout, stripped from project names for display.
# Run from a stage worktree (<main>/.worktrees/<stage>), the main checkout is
# two levels up, so the prefix is the same from either location.
_ROOT = Path(__file__).resolve().parents[1]
if _ROOT.parent.name == ".worktrees":
    _ROOT = _ROOT.parent.parent
PROJECT_PREFIX = re.sub(r"[^A-Za-z0-9]", "-", str(_ROOT.parent)) + "-"


def parse_day(text):
    """Cutoff timestamp for a YYYY-MM-DD date at 00:00 UTC."""
    try:
        day = datetime.datetime.strptime(text, "%Y-%m-%d")
    except ValueError as err:
        raise argparse.ArgumentTypeError(f"expected YYYY-MM-DD, got {text!r}") from err
    return day.replace(tzinfo=datetime.timezone.utc).timestamp()


def parse_args(argv=None):
    p = argparse.ArgumentParser(description=(__doc__ or "").split("\n")[0])
    p.add_argument("days", nargs="?", type=float, default=14.0,
                   help="mtime window in days (default 14)")
    p.add_argument("--since", type=parse_day, metavar="YYYY-MM-DD",
                   help="cutoff at UTC midnight of this date; overrides days")
    p.add_argument("--root", type=Path, default=Path.home() / ".claude/projects", metavar="DIR",
                   help="transcript root (default ~/.claude/projects)")
    return p.parse_args(argv)


def kind_of(proj_name):
    """iv for integration-verify projects, knowledge for knowledge projects, else standard."""
    if "integration-verify" in proj_name:
        return "iv"
    if "knowledge" in proj_name:
        return "knowledge"
    return "standard"


def hit_segment(block):
    """The first command segment of a Bash tool_use that counts as a full run, or None."""
    if not (isinstance(block, dict) and block.get("type") == "tool_use" and block.get("name") == "Bash"):
        return None
    tool_input = block.get("input")
    c = tool_input.get("command", "") if isinstance(tool_input, dict) else ""
    if not isinstance(c, str):
        c = ""
    first = c.split("<<")[0]
    for seg in re.split(r"\n", first):
        m = FULL.search(seg.strip())
        if m and not SCOPED_FLAGS.search(m.group(2) or ""):
            return seg.strip()[:160]
    return None


def result_text(content):
    """Tool result content as text: a string, or the texts of its text items."""
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return "\n".join(i["text"] if isinstance(i.get("text"), str) else "" for i in content
                         if isinstance(i, dict) and i.get("type") == "text")
    return ""


def scan(path):
    """(command, outcome) for each full-run hit in one transcript, read once."""
    hits = []     # (tool_use id, command segment)
    results = {}  # tool_use_id -> result text
    with open(path, encoding="utf-8", errors="replace") as f:
        for line in f:
            try: e = json.loads(line)
            except Exception: continue
            if not isinstance(e, dict): continue
            msg = e.get("message")
            content = msg.get("content") if isinstance(msg, dict) else None
            for b in content if isinstance(content, list) else []:
                if e.get("type") == "assistant":
                    seg = hit_segment(b)
                    if seg: hits.append((b.get("id"), seg))
                elif e.get("type") == "user" and isinstance(b, dict) and b.get("type") == "tool_result":
                    results[b.get("tool_use_id")] = result_text(b.get("content"))
    return [(seg, "blocked" if tid and BLOCKED_MARK in results.get(tid, "") else "ran")
            for tid, seg in hits]


def collect(root, cutoff):
    """One row per hit under root: main transcripts newer than cutoff, plus their subagents."""
    rows = []
    if not root.is_dir():
        return rows
    for proj in root.iterdir():
        if "worktrees" not in proj.name: continue
        name = proj.name.replace(PROJECT_PREFIX, "")
        kind = kind_of(proj.name)
        for main in proj.glob("*.jsonl"):
            if main.stat().st_mtime < cutoff: continue
            sd = proj / main.stem / "subagents"
            files = [("main", main)] + [("sub", s) for s in (sd.glob("*.jsonl") if sd.is_dir() else [])]
            for role, path in files:
                for cmd, outcome in scan(path):
                    rows.append({"name": name, "repo": name.split("--")[0], "kind": kind,
                                 "role": role, "cmd": cmd, "outcome": outcome})
    return rows


def main(argv=None):
    args = parse_args(argv)
    cutoff = args.since if args.since is not None else time.time() - args.days * 86400
    rows = collect(args.root, cutoff)
    counts = Counter((r["repo"], r["kind"], r["role"], r["outcome"]) for r in rows)
    if not counts:
        print("(no hits)")
    for repo, kind, role, outcome in sorted(counts):
        print(f"{repo:40} {kind:9} {role:5} {outcome:8} {counts[(repo, kind, role, outcome)]}")
    for role in ("sub", "main"):
        print(f"--- {role} examples")
        for r in [r for r in rows if r["role"] == role][:25]:
            print(f"  {r['kind']:9} {r['outcome']:8} {r['name']:45} {r['cmd']}")


if __name__ == "__main__":
    main()
