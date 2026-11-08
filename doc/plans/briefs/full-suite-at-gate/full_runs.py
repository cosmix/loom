"""Find likely FULL test-suite runs by stage main agents and their subagents."""
import json, re, sys, time
from pathlib import Path
from collections import Counter

ROOT = Path.home() / ".claude/projects"
cutoff = time.time() - float(sys.argv[1] if len(sys.argv) > 1 else 14) * 86400

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

def scan(path):
    hits = []
    with open(path) as f:
        for line in f:
            try: e = json.loads(line)
            except Exception: continue
            if e.get("type") != "assistant": continue
            for b in (e.get("message") or {}).get("content") or []:
                if isinstance(b, dict) and b.get("type") == "tool_use" and b.get("name") == "Bash":
                    c = (b.get("input") or {}).get("command", "")
                    first = c.split("<<")[0]
                    for seg in re.split(r"\n", first):
                        m = FULL.search(seg.strip())
                        if m and not SCOPED_FLAGS.search(m.group(2) or ""):
                            hits.append(seg.strip()[:160]); break
    return hits

who = Counter(); ex = {"sub": [], "main": []}; per_proj = Counter()
for proj in ROOT.iterdir():
    if "worktrees" not in proj.name: continue
    for main in proj.glob("*.jsonl"):
        if main.stat().st_mtime < cutoff: continue
        name = proj.name.replace("-home-dkaponis-src-", "")
        for h in scan(main):
            who["main"] += 1; ex["main"].append((name, h)); per_proj[(name.split("--")[0], "main")] += 1
        sd = proj / main.stem / "subagents"
        for s in (sd.glob("*.jsonl") if sd.is_dir() else []):
            for h in scan(s):
                who["sub"] += 1; ex["sub"].append((name, h)); per_proj[(name.split("--")[0], "sub")] += 1
print(dict(who)); print(dict(per_proj))
for k in ("sub", "main"):
    print(f"--- {k} examples")
    for n, h in ex[k][:25]: print(f"  {n:45} {h}")
