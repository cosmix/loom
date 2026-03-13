#!/usr/bin/env python3
"""skill-trigger - UserPromptSubmit hook for keyword-based skill suggestions.

Reads user prompts via stdin JSON and outputs skill suggestions to stdout.
Suggestions are injected into Claude's context to encourage skill usage.

Input: JSON from stdin with structure:
  {"session_id": "...", "prompt": "Help me implement JWT authentication"}

Output: JSON with hookSpecificOutput.additionalContext for context injection.
  Plain text stdout from UserPromptSubmit hooks is unreliable (see claude-code#13912).

Dependencies:
  ~/.claude/hooks/loom/skill-keywords.json (built by skill-index-builder.sh)
"""

import json
import os
import re
import sys

INDEX_FILE = os.path.expanduser("~/.claude/hooks/loom/skill-keywords.json")
SKILLS_DIR = os.path.expanduser("~/.claude/skills")
DEBUG_LOG = os.path.expanduser("~/.claude/hooks/loom/skill-trigger.log")
MAX_SUGGESTIONS = 3
MIN_SCORE = 2  # Minimum weighted score to suggest a skill
DEBUG = os.environ.get("LOOM_SKILL_DEBUG", "") == "1"

# Words too generic to be meaningful skill triggers on their own.
# Multi-word keywords containing these are still allowed (e.g. "access control").
STOPWORDS = frozenset({
    # Common programming verbs
    "add", "build", "change", "check", "close", "copy", "create", "debug",
    "delete", "deploy", "find", "fix", "get", "help", "install", "list",
    "make", "move", "open", "pull", "push", "read", "remove", "run", "send",
    "set", "show", "start", "stop", "test", "update", "use", "write",
    # Common nouns
    "app", "bug", "class", "code", "config", "data", "error", "file",
    "function", "issue", "log", "method", "new", "old", "output", "plan",
    "project", "script", "setup", "tool", "type", "value",
    # Project-specific words that appear in nearly every prompt
    "claude", "loom",
})


def _is_name_match(keyword, skill_name):
    """True when keyword strongly identifies the skill by name.

    Exact match ("rust" == "rust") or prefix match with min length 4
    ("refactor" -> "refactoring") to avoid short false matches.
    """
    if keyword == skill_name:
        return True
    if len(keyword) >= 4 and skill_name.startswith(keyword):
        return True
    return False


def _debug(msg):
    """Write debug message to log file if LOOM_SKILL_DEBUG=1."""
    if not DEBUG:
        return
    try:
        import datetime

        ts = datetime.datetime.now().strftime("%H:%M:%S")
        with open(DEBUG_LOG, "a") as f:
            f.write(f"[{ts}] {msg}\n")
    except IOError:
        pass


def main():
    if sys.stdin.isatty():
        _debug("SKIP: stdin is tty")
        return

    try:
        data = json.load(sys.stdin)
    except (json.JSONDecodeError, ValueError):
        _debug("SKIP: invalid JSON on stdin")
        return

    prompt = data.get("prompt", "")
    _debug(f"FIRED: prompt={prompt[:80]!r}")
    if not prompt or not os.path.isfile(INDEX_FILE):
        _debug(f"SKIP: empty prompt or no index (index exists: {os.path.isfile(INDEX_FILE)})")
        return

    try:
        with open(INDEX_FILE) as f:
            index = json.load(f)
    except (json.JSONDecodeError, IOError):
        return

    # Tokenize: lowercase, keep special chars like / - . within words
    words = re.findall(r"[a-z0-9]+(?:[/._-][a-z0-9]+)*", prompt.lower())
    # Filter stopwords from single-word tokens
    tokens = set(w for w in words if len(w) > 1 and w not in STOPWORDS)

    # Generate bigrams and trigrams for multi-word keyword matching
    # e.g. "event sourcing", "api key", "access control"
    # These are NOT stopword-filtered since multi-word phrases are specific enough
    for i in range(len(words) - 1):
        tokens.add(f"{words[i]} {words[i + 1]}")
    for i in range(len(words) - 2):
        tokens.add(f"{words[i]} {words[i + 1]} {words[i + 2]}")

    # Score skills by keyword matches
    # Multi-word matches (containing space) count double since they're more specific
    # Direct skill-name matches get boosted weight (high-confidence signal)
    scores = {}
    matched = {}
    for token in tokens:
        if token in index:
            base_weight = 2 if " " in token else 1
            for skill in index[token]:
                w = base_weight
                # Boost: keyword directly identifies the skill by name
                # e.g., "rust" -> "rust", "refactor" -> "refactoring"
                if base_weight == 1 and _is_name_match(token, skill):
                    w = 2
                scores[skill] = scores.get(skill, 0) + w
                matched.setdefault(skill, []).append(token)

    if not scores:
        return

    # Filter skills below minimum score threshold
    qualified = {s: sc for s, sc in scores.items() if sc >= MIN_SCORE}
    if not qualified:
        return

    # Sort by score descending, take top N
    top = sorted(qualified.items(), key=lambda x: -x[1])[:MAX_SUGGESTIONS]

    lines = []
    for skill, _score in top:
        kws = ", ".join(matched[skill][:4])
        desc = _get_description(skill)
        if desc:
            lines.append(f"  - /{skill} -- {desc} (matched: {kws})")
        else:
            lines.append(f"  - /{skill} (matched: {kws})")

    if lines:
        context = (
            "SKILL MATCH: These skills are relevant to this task."
            ' Invoke the best match with Skill(skill="name") before implementing:\n'
        )
        context += "\n".join(lines)
        _debug(f"SUGGEST: {context}")
        # Use JSON additionalContext format — plain text stdout is unreliable
        # for UserPromptSubmit hooks (see claude-code#13912)
        print(json.dumps({
            "hookSpecificOutput": {
                "hookEventName": "UserPromptSubmit",
                "additionalContext": context,
            }
        }))
    elif DEBUG:
        _debug(f"NO MATCH: scores={scores}")


def _get_description(skill_name):
    """Extract short description from SKILL.md frontmatter."""
    path = os.path.join(SKILLS_DIR, skill_name, "SKILL.md")
    if not os.path.isfile(path):
        return ""
    try:
        with open(path) as f:
            text = f.read(2000)
        m = re.search(r"^---\s*\n(.*?)\n---", text, re.DOTALL)
        if not m:
            return ""
        fm = m.group(1)
        # Multiline description (|)
        m = re.search(r"^description:\s*\|\s*\n\s+(.+)", fm, re.MULTILINE)
        if m:
            d = m.group(1).strip()
        else:
            # Inline description
            m = re.search(r"^description:\s*(.+)", fm, re.MULTILINE)
            if not m:
                return ""
            d = m.group(1).strip()
        # Truncate at first natural break point
        for marker in [". Trigger", ". Use when", ". Covers", ". Keywords", ". Primary"]:
            idx = d.find(marker)
            if 0 < idx < 80:
                d = d[: idx + 1]
                break
        return d[:80]
    except IOError:
        return ""


if __name__ == "__main__":
    main()
