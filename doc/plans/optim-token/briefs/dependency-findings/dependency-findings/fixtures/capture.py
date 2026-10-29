"""Capture the dependency-findings harness inputs for one recorded pair.

Usage: python3 -I capture.py <repo> <plan.md> <dep-id> <successor-id> <transcript-dir> <journal> <out.json>

Reads only: the archived plan YAML, git history, the archived journal and the
successor's transcripts. Writes only <out.json>.

Recorded on 2026-10-18 at 6fee4ecd from the repository root, with
P=~/.claude/projects/-home-dkaponis-src-loom--worktrees- and
A=.loom/memory/archive:
  capture.py . doc/plans/archive/DONE-PLAN-target-ref-guard.md guard-core guard-integration \
    ${P}guard-integration $A/PLAN-target-ref-guard-20261003T180511Z/memory/guard-core.md \
    guard-core--guard-integration.json
  capture.py . doc/plans/archive/DONE-PLAN-token-flow.md token-core tui-tokens \
    ${P}tui-tokens $A/PLAN-token-flow-20261014T220227Z/memory/token-core.md token-core--tui-tokens.json
  capture.py . doc/plans/archive/DONE-PLAN-token-flow.md token-core web-tokens \
    ${P}web-tokens $A/PLAN-token-flow-20261014T220227Z/memory/token-core.md token-core--web-tokens.json
guard-core.md and token-core.md are byte copies of those two archived journals.
Totals printed: guard-integration 17 transcripts, 71,574 first-arrival and
2,395,454 replay-weighted tokens; tui-tokens 9, 5,909 and 79,954; web-tokens
11, 2,998 and 23,264. Every recorded spawn matched its subagent transcript.

Definitions (recorded in the output's "method" field):
- A transcript is every top-level session of the successor's worktree slug and
  every subagents/agent-*.jsonl beneath it, except adjudication sessions.
- A read is a Read, Grep, Glob or Bash tool call whose input names, as one
  path token, a file the dependency's merge changed. Tokens are split on
  whitespace and quoting; an absolute path is made repo-relative by dropping
  everything up to `.worktrees/<id>/` or the repository root; a relative path
  is taken as repo-relative when it names a changed file, else joined to the
  successor's working_dir with `..` resolved; a trailing `:<line>` or
  `:<start>-<end>` is dropped.
- A call's tokens are its tool-result characters divided by 4. A call naming
  k changed files is split equally among them.
- first_arrival_tokens sums each call once; replay_weighted_tokens multiplies
  each call by the API calls (distinct assistant message ids) that first
  appear after its result line in the same transcript.
"""

import json
import os
import posixpath
import re
import subprocess
import sys
from collections import defaultdict

import yaml

READ_TOOLS = {"Read", "Grep", "Glob", "Bash"}
SPLIT = re.compile(r"[\s\"'`=(),;|<>]+")
LINE_SUFFIX = re.compile(r":\d+(-\d+)?$")


def git(repo, *args):
    return subprocess.run(["git", *args], cwd=repo, check=True, capture_output=True, text=True).stdout


def plan_stages(plan_path):
    text = open(plan_path, encoding="utf-8").read()
    block = text.split("```yaml\n", 1)[1].split("\n```", 1)[0]
    return {s["id"]: s for s in yaml.safe_load(block)["loom"]["stages"]}


def stage_record(stage):
    return {
        "id": stage["id"],
        "name": stage.get("name", stage["id"]),
        "stage_type": stage.get("stage_type", "standard"),
        "working_dir": stage.get("working_dir", "."),
        "description": stage.get("description", ""),
        "files": stage.get("files", []) or [],
        "artifacts": stage.get("artifacts", []) or [],
        "wiring_descriptions": [w.get("description", "") for w in stage.get("wiring", []) or []],
        "dependencies": stage.get("dependencies", []) or [],
    }


def merge_of(repo, stage_id):
    line = git(repo, "log", "--all", "--merges", "--format=%H %P", f"--grep=Merge loom/{stage_id} into")
    rows = [row.split() for row in line.strip().splitlines()]
    return rows


def normalize(token, working_dir, changed):
    token = LINE_SUFFIX.sub("", token.strip())
    if not token or "*" in token:
        return None
    if token.startswith("/"):
        marker = "/.worktrees/"
        if marker in token:
            rest = token.split(marker, 1)[1]
            token = rest.split("/", 1)[1] if "/" in rest else ""
        elif token.startswith(REPO_ROOT + "/"):
            token = token[len(REPO_ROOT) + 1:]
        else:
            return None
        return token if token in changed else None
    token = token[2:] if token.startswith("./") else token
    if token in changed:
        return token
    joined = posixpath.normpath(posixpath.join(working_dir, token)) if working_dir != "." else posixpath.normpath(token)
    return joined if joined in changed else None


def result_text(block):
    content = block.get("content")
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return "".join(part.get("text", "") for part in content if isinstance(part, dict) and part.get("type") == "text")
    return ""


def first_user_text(rows):
    for row in rows:
        if row.get("type") == "user":
            content = row.get("message", {}).get("content")
            return content if isinstance(content, str) else json.dumps(content)
    return ""


def analyse(path, working_dir, changed):
    rows = []
    for line in open(path, encoding="utf-8"):
        try:
            rows.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    first_seen = {}
    uses = {}
    spawns = {}
    results = []
    for index, row in enumerate(rows):
        message = row.get("message") or {}
        if row.get("type") == "assistant":
            first_seen.setdefault(message.get("id"), index)
            for block in message.get("content") or []:
                if isinstance(block, dict) and block.get("type") == "tool_use":
                    uses[block["id"]] = (block["name"], block.get("input") or {})
        elif row.get("type") == "user" and isinstance(message.get("content"), list):
            for block in message["content"]:
                if isinstance(block, dict) and block.get("type") == "tool_result":
                    results.append((index, block))
                    use = uses.get(block.get("tool_use_id"))
                    extra = row.get("toolUseResult")
                    if use and use[0] in ("Agent", "Task") and isinstance(extra, dict) and extra.get("agentId"):
                        spawns[extra["agentId"]] = {
                            "subagent_type": use[1].get("subagent_type", ""),
                            "description": use[1].get("description", ""),
                            "prompt": use[1].get("prompt", ""),
                        }
    starts = sorted(first_seen.values())
    reads = defaultdict(lambda: {"calls": 0, "first_arrival_tokens": 0.0, "replay_weighted_tokens": 0.0})
    for index, block in results:
        use = uses.get(block.get("tool_use_id"))
        if not use or use[0] not in READ_TOOLS:
            continue
        named = set()
        for token in SPLIT.split(json.dumps(use[1], ensure_ascii=False).replace("\\n", " ")):
            hit = normalize(token, working_dir, changed)
            if hit:
                named.add(hit)
        if not named:
            continue
        tokens = len(result_text(block)) / 4
        after = sum(1 for start in starts if start > index)
        for hit in named:
            entry = reads[hit]
            entry["calls"] += 1
            entry["first_arrival_tokens"] += tokens / len(named)
            entry["replay_weighted_tokens"] += tokens * after / len(named)
    out = [
        {"path": key, "calls": value["calls"], "first_arrival_tokens": round(value["first_arrival_tokens"]),
         "replay_weighted_tokens": round(value["replay_weighted_tokens"])}
        for key, value in sorted(reads.items())
    ]
    return rows, len(first_seen), out, spawns


def main():
    global REPO_ROOT
    repo, plan, dep_id, succ_id, tdir, journal, out_path = sys.argv[1:8]
    REPO_ROOT = os.path.abspath(repo)
    stages = plan_stages(plan)
    dep = stage_record(stages[dep_id])
    succ = stage_record(stages[succ_id])
    dep_merges = merge_of(repo, dep_id)
    succ_merges = merge_of(repo, succ_id)
    # The recorded run is the one whose successor forked from the dependency's merge.
    chosen = None
    for m, p1, p2 in dep_merges:
        for sm, sp1, sp2 in succ_merges:
            if git(repo, "merge-base", sp1, sp2).strip() == m:
                chosen = (m, p1, p2)
    m, p1, completed = chosen
    change_base = git(repo, "merge-base", p1, completed).strip()
    changed = sorted(p for p in git(repo, "diff", "-z", "--name-only", "--no-renames", change_base, completed).split("\0") if p)
    changed_set = set(changed)
    dep.update({"completed_commit": completed, "merge_commit": m, "change_base": change_base,
                "journal": os.path.basename(journal)})
    succ["base_commit"] = m
    transcripts = []
    spawns = {}
    files = []
    for root, _, names in os.walk(tdir):
        for name in names:
            if name.endswith(".jsonl"):
                files.append(os.path.join(root, name))
    analysed = {}
    for path in sorted(files):
        rows, api_calls, reads, found = analyse(path, succ["working_dir"], changed_set)
        spawns.update(found)
        analysed[path] = (rows, api_calls, reads)
    for path in sorted(files):
        rows, api_calls, reads = analysed[path]
        head = first_user_text(rows)
        stem = os.path.basename(path)[:-len(".jsonl")]
        if "/subagents/" in path:
            role = "worker"
            agent = stem[len("agent-"):]
            spawn = spawns.get(agent)
        elif head.startswith("Read the adjudication signal"):
            continue
        elif head.startswith("You are the contract test writer"):
            role, spawn = "contract", None
        else:
            role, spawn = "main", None
        transcripts.append({"id": stem, "role": role, "api_calls": api_calls, "spawn": spawn, "reads": reads})
    totals = {
        "first_arrival_tokens": sum(r["first_arrival_tokens"] for t in transcripts for r in t["reads"]),
        "replay_weighted_tokens": sum(r["replay_weighted_tokens"] for t in transcripts for r in t["reads"]),
    }
    doc = {
        "pair": f"{dep_id}->{succ_id}",
        "method": __doc__.split("Definitions", 1)[1].strip(),
        "dependency": dep,
        "successor": succ,
        "changed_paths": changed,
        "transcripts": transcripts,
        "totals": totals,
    }
    with open(out_path, "w", encoding="utf-8") as handle:
        json.dump(doc, handle, indent=1, ensure_ascii=False)
        handle.write("\n")
    print(out_path, len(transcripts), "transcripts", totals, "spawns matched",
          sum(1 for t in transcripts if t["role"] == "worker" and t["spawn"]), "/",
          sum(1 for t in transcripts if t["role"] == "worker"))


REPO_ROOT = ""
if __name__ == "__main__":
    main()
