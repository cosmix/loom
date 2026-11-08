"""Audit verification commands run by stage main agents vs their subagents."""
import json, os, re, sys, time
from pathlib import Path
from collections import Counter, defaultdict

ROOT = Path.home() / ".claude/projects"
DAYS = float(sys.argv[1]) if len(sys.argv) > 1 else 14
cutoff = time.time() - DAYS * 86400

VERIFY = re.compile(
    r"\b(cargo (test|clippy|fmt|check|build|nextest)|vitest|bun (run )?(test|lint|build|typecheck)|bunx (vitest|tsc|eslint|biome)|tsc\b|pytest|ruff|mypy|eslint|go test|go vet|loom check|loom stage complete|markdownlint|uv run (pytest|ruff|mypy))"
)

def classify(cmd):
    c = cmd
    if "loom stage complete" in c: return "loom-complete"
    if "loom check" in c:
        return "loom-check-criterion" if "--criterion" in c else "loom-check"
    if re.search(r"cargo (fmt)|rustfmt", c): return "fmt"
    if re.search(r"cargo clippy|eslint|ruff|biome|bun (run )?lint", c): return "lint"
    if re.search(r"cargo (check|build)|tsc\b|bun (run )?build|typecheck", c): return "compile"
    if re.search(r"cargo (test|nextest)|vitest|bun (run )?test|pytest|go test", c): return "test"
    return "other"

def cmds(path):
    out = []
    try:
        with open(path) as f:
            for line in f:
                try: e = json.loads(line)
                except Exception: continue
                if e.get("type") != "assistant": continue
                for b in (e.get("message") or {}).get("content") or []:
                    if isinstance(b, dict) and b.get("type") == "tool_use" and b.get("name") == "Bash":
                        c = (b.get("input") or {}).get("command", "")
                        if VERIFY.search(c):
                            out.append((e.get("timestamp", ""), c))
    except Exception:
        pass
    return out

stats = defaultdict(Counter)
examples = []
sessions = 0
for proj in ROOT.iterdir():
    if "worktrees" not in proj.name or "integration-verify" in proj.name or "knowledge" in proj.name:
        continue
    for main in proj.glob("*.jsonl"):
        if main.stat().st_mtime < cutoff: continue
        sub_dir = proj / main.stem / "subagents"
        subs = list(sub_dir.glob("*.jsonl")) if sub_dir.is_dir() else []
        if not subs: continue
        sessions += 1
        events = [(t, "main", c) for t, c in cmds(main)]
        for s in subs:
            events += [(t, "sub", c) for t, c in cmds(s)]
        events.sort()
        for t, who, c in events:
            stats[who][classify(c)] += 1
        examples.append((proj.name.split("worktrees-")[-1], main.stem[:8], events))

print(f"standard-stage sessions with subagents, last {DAYS:g}d: {sessions}")
for who in ("sub", "main"):
    print(who, dict(stats[who].most_common()))
print()
# Per-session timeline, condensed
for stage, sid, ev in sorted(examples, key=lambda x: x[2][0][0] if x[2] else "")[-int(os.environ.get("N", "8")):]:
    print(f"=== {stage} {sid}")
    for t, who, c in ev:
        print(f"  {t[11:19]} {who:4} {classify(c):20} {c.splitlines()[0][:150]}")
